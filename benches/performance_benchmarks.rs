use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use git_warp::cow::{clone_directory, is_cow_supported};
use git_warp::git::GitRepository;
use git_warp::process::ProcessManager;
use git_warp::rewrite::PathRewriter;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn bench_cow_vs_traditional_copy(c: &mut Criterion) {
    let mut group = c.benchmark_group("cow_vs_traditional");

    // Test with different repository sizes
    let sizes = vec![
        ("small", 10),   // 10 files
        ("medium", 100), // 100 files
        ("large", 1000), // 1000 files
    ];

    for (size_name, file_count) in sizes {
        let temp_dir = create_test_repo_with_files(file_count);
        let repo_path = temp_dir.path();

        // Benchmark CoW clone
        group.bench_with_input(
            BenchmarkId::new("cow_clone", size_name),
            &repo_path,
            |b, repo_path| {
                b.iter(|| {
                    let clone_path = repo_path
                        .parent()
                        .unwrap()
                        .join(format!("cow_clone_{}", rand::random::<u32>()));
                    let _ = clone_directory(repo_path, &clone_path);
                    // Clean up
                    let _ = fs::remove_dir_all(&clone_path);
                })
            },
        );

        // Benchmark traditional copy
        group.bench_with_input(
            BenchmarkId::new("traditional_copy", size_name),
            &repo_path,
            |b, repo_path| {
                b.iter(|| {
                    let copy_path = repo_path
                        .parent()
                        .unwrap()
                        .join(format!("trad_copy_{}", rand::random::<u32>()));
                    let _ = copy_directory_recursive(repo_path, &copy_path);
                    // Clean up
                    let _ = fs::remove_dir_all(&copy_path);
                })
            },
        );
    }

    group.finish();
}

fn bench_git_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("git_operations");

    let temp_dir = setup_git_repository();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    // Benchmark worktree listing
    group.bench_function("list_worktrees", |b| {
        b.iter(|| {
            let _ = git_repo.list_worktrees();
        })
    });

    // Benchmark worktree creation
    group.bench_function("create_worktree", |b| {
        b.iter(|| {
            let branch_name = format!("bench-branch-{}", rand::random::<u32>());
            let worktree_path = repo_path.join("worktrees").join(&branch_name);

            let _ = git_repo.create_worktree_and_branch(&branch_name, &worktree_path, None);

            // Clean up
            let _ = git_repo.remove_worktree(&worktree_path, false, true);
        })
    });

    // Benchmark branch analysis
    group.bench_function("analyze_branches", |b| {
        // Create some test worktrees first
        let mut test_worktrees = Vec::new();
        for i in 0..5 {
            let branch_name = format!("analysis-branch-{}", i);
            let worktree_path = repo_path.join("worktrees").join(&branch_name);

            if git_repo
                .create_worktree_and_branch(&branch_name, &worktree_path, None)
                .is_ok()
            {
                test_worktrees.push(git_warp::git::WorktreeInfo {
                    path: worktree_path,
                    branch: branch_name,
                    head: "HEAD".to_string(),
                    is_primary: false,
                });
            }
        }

        b.iter(|| {
            let _ = git_repo.analyze_branches_for_cleanup(&test_worktrees);
        });

        // Clean up test worktrees
        for worktree in &test_worktrees {
            let _ = git_repo.remove_worktree(&worktree.path, false, true);
        }
    });

    group.finish();
}

fn bench_process_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("process_detection");

    let temp_dir = tempdir().unwrap();
    let test_path = temp_dir.path();

    // Benchmark process scanning
    group.bench_function("scan_directory", |b| {
        let mut manager = ProcessManager::new();
        b.iter(|| {
            let _ = manager.find_processes_in_directory(test_path);
        })
    });

    // Benchmark system refresh
    group.bench_function("refresh_system", |b| {
        let mut manager = ProcessManager::new();
        b.iter(|| {
            manager.refresh();
        })
    });

    group.finish();
}

fn bench_path_rewriting(c: &mut Criterion) {
    let mut group = c.benchmark_group("path_rewriting");

    // Test with different numbers of files
    let file_counts = vec![10, 50, 100, 500];

    for file_count in file_counts {
        let temp_dir = create_repo_with_config_files(file_count);
        let repo_path = temp_dir.path();
        let clone_path = repo_path.parent().unwrap().join("clone");

        // Create clone for rewriting
        copy_directory_recursive(repo_path, &clone_path).unwrap();

        group.bench_with_input(
            BenchmarkId::new("rewrite_paths", file_count),
            &clone_path,
            |b, clone_path| {
                b.iter(|| {
                    let rewriter = PathRewriter::new(
                        repo_path.to_string_lossy(),
                        clone_path.to_string_lossy(),
                    );
                    let _ = rewriter.rewrite_paths();
                })
            },
        );

        fs::remove_dir_all(&clone_path).unwrap();
    }

    group.finish();
}

fn bench_configuration_loading(c: &mut Criterion) {
    let mut group = c.benchmark_group("configuration");

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Create a complex configuration file
    let config_content = r#"
terminal_mode = "tab"
use_cow = true
auto_confirm = false

[git]
default_branch = "main"
auto_fetch = true
auto_prune = true

[process]
check_processes = true
auto_kill = false
kill_timeout = 5

[terminal]
app = "auto"
auto_activate = true
init_commands = ["npm install", "source .env", "echo setup"]

[agent]
enabled = true
refresh_rate = 1000
max_activities = 100
claude_hooks = true
"#;

    fs::write(&config_path, config_content).unwrap();

    group.bench_function("load_config", |b| {
        use git_warp::config::ConfigManager;
        b.iter(|| {
            let manager = ConfigManager {
                config: git_warp::config::Config::default(),
                config_path: config_path.clone(),
            };
            let _ = manager.get();
        })
    });

    group.bench_function("serialize_config", |b| {
        use git_warp::config::Config;
        let config = Config::default();
        b.iter(|| {
            let _ = toml::to_string(&config);
        })
    });

    group.finish();
}

fn bench_filesystem_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("filesystem");

    // Test CoW support detection
    group.bench_function("cow_support_detection", |b| {
        b.iter(|| {
            let _ = is_cow_supported(".");
        })
    });

    // Test directory traversal
    let temp_dir = create_nested_directory_structure();
    let test_path = temp_dir.path();

    group.bench_function("directory_traversal", |b| {
        b.iter(|| {
            let _ = count_files_recursive(test_path);
        })
    });

    group.finish();
}

// Helper functions for benchmarks

fn create_test_repo_with_files(file_count: usize) -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    // Create many files
    for i in 0..file_count {
        let content = format!("File {} content with some text to make it realistic", i);
        fs::write(repo_path.join(format!("file_{}.txt", i)), content).unwrap();
    }

    // Create some directories
    for i in 0..file_count / 10 {
        let dir_path = repo_path.join(format!("dir_{}", i));
        fs::create_dir_all(&dir_path).unwrap();

        for j in 0..5 {
            fs::write(
                dir_path.join(format!("nested_{}.txt", j)),
                format!("Nested file {} content", j),
            )
            .unwrap();
        }
    }

    temp_dir
}

fn create_repo_with_config_files(file_count: usize) -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    // Create .gitignore
    fs::write(
        repo_path.join(".gitignore"),
        "*.tmp\nnode_modules/\nbuild/\n",
    )
    .unwrap();

    // Create node_modules (gitignored files)
    let node_modules = repo_path.join("node_modules");
    fs::create_dir_all(&node_modules).unwrap();

    for i in 0..file_count {
        let config_content = format!(
            r#"{{
  "name": "package-{}",
  "path": "{}",
  "build_dir": "{}/build"
}}"#,
            i,
            repo_path.display(),
            repo_path.display()
        );

        fs::write(
            node_modules.join(format!("config_{}.json", i)),
            config_content,
        )
        .unwrap();
    }

    temp_dir
}

fn create_nested_directory_structure() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create deeply nested structure
    for i in 0..10 {
        let level1 = base_path.join(format!("level1_{}", i));
        fs::create_dir_all(&level1).unwrap();

        for j in 0..5 {
            let level2 = level1.join(format!("level2_{}", j));
            fs::create_dir_all(&level2).unwrap();

            for k in 0..3 {
                fs::write(level2.join(format!("file_{}.txt", k)), "content").unwrap();
            }
        }
    }

    temp_dir
}

fn setup_git_repository() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(&["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(&["config", "user.email", "bench@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(&["config", "user.name", "Bench User"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    fs::write(repo_path.join("README.md"), "# Benchmark Repo").unwrap();

    Command::new("git")
        .args(&["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(&["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    temp_dir
}

fn copy_directory_recursive<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> std::io::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_directory_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

fn count_files_recursive<P: AsRef<Path>>(dir: P) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                count += 1;
            } else if path.is_dir() {
                count += count_files_recursive(&path);
            }
        }
    }
    count
}

criterion_group!(
    benches,
    bench_cow_vs_traditional_copy,
    bench_git_operations,
    bench_process_detection,
    bench_path_rewriting,
    bench_configuration_loading,
    bench_filesystem_operations
);

criterion_main!(benches);
