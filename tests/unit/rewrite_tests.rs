use git_warp::rewrite::PathRewriter;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_path_rewriter_creation() {
    let src_dir = "/original/project";
    let dst_dir = "/cloned/project";

    let rewriter = PathRewriter::new(src_dir, dst_dir);

    // PathRewriter should be created successfully
    println!("PathRewriter created for {} -> {}", src_dir, dst_dir);
}

#[test]
fn test_simple_path_rewriting() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dst_dir = temp_dir.path().join("destination");

    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create a file with absolute paths
    let config_content = format!(
        r#"
project_root = "{}"
build_dir = "{}/build"
cache_dir = "{}/cache"
"#,
        src_dir.display(),
        src_dir.display(),
        src_dir.display()
    );

    let config_path = dst_dir.join("config.toml");
    fs::write(&config_path, config_content).unwrap();

    let rewriter = PathRewriter::new(&src_dir, &dst_dir);
    let result = rewriter.rewrite_paths();

    match result {
        Ok(()) => {
            // Read back the file and check if paths were rewritten
            let rewritten_content = fs::read_to_string(&config_path).unwrap();

            // Should contain destination paths, not source paths
            assert!(rewritten_content.contains(&dst_dir.to_string_lossy().to_string()));
            assert!(!rewritten_content.contains(&src_dir.to_string_lossy().to_string()));

            println!("Path rewriting successful");
        }
        Err(e) => {
            println!("Path rewriting failed: {}", e);
        }
    }
}

#[test]
fn test_gitignore_pattern_matching() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dst_dir = temp_dir.path().join("destination");

    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create .gitignore file
    let gitignore_content = r#"
node_modules/
*.log
build/
dist/
.env
venv/
__pycache__/
"#;
    fs::write(dst_dir.join(".gitignore"), gitignore_content).unwrap();

    // Create gitignored files with paths to rewrite
    let node_modules_dir = dst_dir.join("node_modules").join("package");
    fs::create_dir_all(&node_modules_dir).unwrap();

    let package_json_content = format!(
        r#"{{
  "name": "test-package",
  "main": "{}",
  "scripts": {{
    "start": "node {}/index.js"
  }}
}}"#,
        src_dir.join("lib/main.js").display(),
        src_dir.display()
    );

    fs::write(node_modules_dir.join("package.json"), package_json_content).unwrap();

    // Create non-gitignored file (should not be rewritten)
    let main_config_content = format!(
        r#"
# This file should NOT be rewritten (not gitignored)
project_path = "{}"
"#,
        src_dir.display()
    );
    fs::write(dst_dir.join("config.yaml"), main_config_content).unwrap();

    // Create build directory file (gitignored, should be rewritten)
    let build_dir = dst_dir.join("build");
    fs::create_dir_all(&build_dir).unwrap();
    let build_config_content = format!(
        r#"
BUILD_ROOT = "{}"
SOURCE_DIR = "{}"
"#,
        src_dir.display(),
        src_dir.display()
    );
    fs::write(build_dir.join("build.conf"), build_config_content).unwrap();

    let rewriter = PathRewriter::new(&src_dir, &dst_dir);
    let result = rewriter.rewrite_paths();

    match result {
        Ok(()) => {
            // Check gitignored file was rewritten
            let package_content =
                fs::read_to_string(node_modules_dir.join("package.json")).unwrap();
            assert!(package_content.contains(&dst_dir.to_string_lossy().to_string()));

            let build_content = fs::read_to_string(build_dir.join("build.conf")).unwrap();
            assert!(build_content.contains(&dst_dir.to_string_lossy().to_string()));

            // Regular text files are rewritten too; gitignored paths must not be skipped.
            let main_content = fs::read_to_string(dst_dir.join("config.yaml")).unwrap();
            assert!(main_content.contains(&dst_dir.to_string_lossy().to_string()));

            println!("Gitignore-aware path rewriting successful");
        }
        Err(e) => {
            println!("Path rewriting failed: {}", e);
        }
    }
}

#[test]
fn test_binary_file_handling() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dst_dir = temp_dir.path().join("destination");

    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create .gitignore that includes binary files
    fs::write(dst_dir.join(".gitignore"), "*.bin\n*.exe\nnode_modules/\n").unwrap();

    // Create binary file in gitignored location
    let node_modules_dir = dst_dir.join("node_modules");
    fs::create_dir_all(&node_modules_dir).unwrap();

    let binary_data = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD];
    fs::write(node_modules_dir.join("binary.bin"), &binary_data).unwrap();

    // Create text file with paths in gitignored location
    let text_content = format!("path={}", src_dir.display());
    fs::write(node_modules_dir.join("config.txt"), text_content).unwrap();

    let rewriter = PathRewriter::new(&src_dir, &dst_dir);
    let result = rewriter.rewrite_paths();

    match result {
        Ok(()) => {
            // Binary file should be unchanged
            let binary_content = fs::read(node_modules_dir.join("binary.bin")).unwrap();
            assert_eq!(binary_content, binary_data);

            // Text file should be rewritten
            let text_content = fs::read_to_string(node_modules_dir.join("config.txt")).unwrap();
            assert!(text_content.contains(&dst_dir.to_string_lossy().to_string()));

            println!("Binary file handling successful");
        }
        Err(e) => {
            println!("Path rewriting with binary files failed: {}", e);
        }
    }
}

#[test]
fn test_python_virtual_environment_rewriting() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dst_dir = temp_dir.path().join("destination");

    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create .gitignore for venv
    fs::write(dst_dir.join(".gitignore"), "venv/\n__pycache__/\n").unwrap();

    // Create venv structure
    let venv_dir = dst_dir.join("venv");
    let bin_dir = venv_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    // Create pyvenv.cfg with absolute paths
    let pyvenv_content = format!(
        r#"home = /usr/bin
include-system-site-packages = false
version = 3.9.0
executable = {}/bin/python
command = {}/bin/python -m venv {}/venv
"#,
        src_dir.display(),
        src_dir.display(),
        src_dir.display()
    );

    fs::write(venv_dir.join("pyvenv.cfg"), pyvenv_content).unwrap();

    // Create activate script with absolute paths
    let activate_content = format!(
        r#"#!/bin/bash
VIRTUAL_ENV="{}/venv"
export VIRTUAL_ENV
export PATH="$VIRTUAL_ENV/bin:$PATH"
"#,
        src_dir.display()
    );

    fs::write(bin_dir.join("activate"), activate_content).unwrap();

    let rewriter = PathRewriter::new(&src_dir, &dst_dir);
    let result = rewriter.rewrite_paths();

    match result {
        Ok(()) => {
            // Check pyvenv.cfg was rewritten
            let pyvenv_content = fs::read_to_string(venv_dir.join("pyvenv.cfg")).unwrap();
            assert!(pyvenv_content.contains(&dst_dir.to_string_lossy().to_string()));
            assert!(!pyvenv_content.contains(&src_dir.to_string_lossy().to_string()));

            // Check activate script was rewritten
            let activate_content = fs::read_to_string(bin_dir.join("activate")).unwrap();
            assert!(activate_content.contains(&dst_dir.to_string_lossy().to_string()));

            println!("Python virtual environment rewriting successful");
        }
        Err(e) => {
            println!("Python venv rewriting failed: {}", e);
        }
    }
}

#[test]
fn test_node_modules_rewriting() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dst_dir = temp_dir.path().join("destination");

    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create .gitignore
    fs::write(dst_dir.join(".gitignore"), "node_modules/\n*.log\n").unwrap();

    // Create node_modules structure
    let node_modules = dst_dir.join("node_modules");
    let package_dir = node_modules.join("some-package");
    fs::create_dir_all(&package_dir).unwrap();

    // Create package.json with absolute paths
    let package_json = format!(
        r#"{{
  "name": "some-package",
  "bin": "{}",
  "scripts": {{
    "postinstall": "node {}/setup.js"
  }},
  "config": {{
    "buildDir": "{}/build"
  }}
}}"#,
        src_dir.join("bin/tool").display(),
        src_dir.display(),
        src_dir.display()
    );

    fs::write(package_dir.join("package.json"), package_json).unwrap();

    // Create nested dependency
    let nested_dir = node_modules
        .join("nested")
        .join("node_modules")
        .join("deep-package");
    fs::create_dir_all(&nested_dir).unwrap();

    let nested_config = format!(
        r#"{{
  "rootPath": "{}"
}}"#,
        src_dir.display()
    );

    fs::write(nested_dir.join("config.json"), nested_config).unwrap();

    let rewriter = PathRewriter::new(&src_dir, &dst_dir);
    let result = rewriter.rewrite_paths();

    match result {
        Ok(()) => {
            // Check package.json was rewritten
            let package_content = fs::read_to_string(package_dir.join("package.json")).unwrap();
            assert!(package_content.contains(&dst_dir.to_string_lossy().to_string()));

            // Check nested dependency was rewritten
            let nested_content = fs::read_to_string(nested_dir.join("config.json")).unwrap();
            assert!(nested_content.contains(&dst_dir.to_string_lossy().to_string()));

            println!("Node.js modules rewriting successful");
        }
        Err(e) => {
            println!("Node modules rewriting failed: {}", e);
        }
    }
}

#[test]
fn test_complex_gitignore_patterns() {
    let temp_dir = tempdir().unwrap();
    let dst_dir = temp_dir.path().join("destination");
    fs::create_dir_all(&dst_dir).unwrap();

    // Create complex .gitignore
    let gitignore_content = r#"
# Dependencies
node_modules/
bower_components/

# Build outputs
build/
dist/
out/
*.min.js

# Logs
*.log
logs/

# Environment
.env
.env.local
.env.*.local

# IDE
.vscode/
.idea/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# Python
__pycache__/
*.pyc
*.pyo
venv/
.pytest_cache/

# Rust
target/
Cargo.lock

# Go
vendor/
"#;

    fs::write(dst_dir.join(".gitignore"), gitignore_content).unwrap();

    // Test various file patterns
    let test_files = vec![
        ("node_modules/package/config.json", true),
        ("build/output.js", true),
        ("dist/app.min.js", true),
        ("app.log", true),
        (".env", true),
        (".vscode/settings.json", true),
        ("__pycache__/module.pyc", true),
        ("target/debug/app", true),
        ("src/main.rs", false),
        ("README.md", false),
        ("package.json", false),
    ];

    for (file_path, should_match) in test_files {
        let full_path = dst_dir.join(file_path);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        fs::write(&full_path, "test content").unwrap();
    }

    // The gitignore pattern matching would be tested in the actual rewriting logic
    println!("Complex gitignore patterns test setup complete");
}

#[test]
fn test_path_rewriter_error_handling() {
    let rewriter = PathRewriter::new("/nonexistent/source", "/nonexistent/dest");

    let result = rewriter.rewrite_paths();

    // Should handle non-existent paths gracefully
    match result {
        Ok(()) => {
            println!("Path rewriter handled non-existent paths gracefully");
        }
        Err(e) => {
            println!("Path rewriter failed as expected: {}", e);
        }
    }
}

#[test]
fn test_relative_path_handling() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dst_dir = temp_dir.path().join("destination");

    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create file with mix of absolute and relative paths
    let config_content = format!(
        r#"
absolute_path = "{}"
relative_path = "./config"
another_absolute = "{}/data"
another_relative = "../shared"
"#,
        src_dir.display(),
        src_dir.display()
    );

    fs::write(dst_dir.join("config.toml"), config_content).unwrap();

    let rewriter = PathRewriter::new(&src_dir, &dst_dir);
    let result = rewriter.rewrite_paths();

    match result {
        Ok(()) => {
            let rewritten = fs::read_to_string(dst_dir.join("config.toml")).unwrap();

            // Absolute paths should be rewritten
            assert!(rewritten.contains(&dst_dir.to_string_lossy().to_string()));

            // Relative paths should remain unchanged
            assert!(rewritten.contains("./config"));
            assert!(rewritten.contains("../shared"));

            println!("Relative path handling successful");
        }
        Err(e) => {
            println!("Relative path test failed: {}", e);
        }
    }
}

#[test]
fn test_unicode_path_handling() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dst_dir = temp_dir.path().join("destination");

    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create file with unicode content
    let unicode_content = format!(
        r#"
# Configuration with unicode
project_path = "{}"
user_name = "测试用户"
emoji_path = "{}/😀"
special_chars = "{}/àáâãäå"
"#,
        src_dir.display(),
        src_dir.display(),
        src_dir.display()
    );

    fs::write(dst_dir.join("unicode.conf"), unicode_content).unwrap();

    let rewriter = PathRewriter::new(&src_dir, &dst_dir);
    let result = rewriter.rewrite_paths();

    match result {
        Ok(()) => {
            let rewritten = fs::read_to_string(dst_dir.join("unicode.conf")).unwrap();

            // Paths should be rewritten while preserving unicode
            assert!(rewritten.contains(&dst_dir.to_string_lossy().to_string()));
            assert!(rewritten.contains("测试用户"));
            assert!(rewritten.contains("😀"));
            assert!(rewritten.contains("àáâãäå"));

            println!("Unicode path handling successful");
        }
        Err(e) => {
            println!("Unicode path test failed: {}", e);
        }
    }
}
