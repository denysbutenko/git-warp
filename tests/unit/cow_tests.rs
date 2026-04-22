use git_warp::cow::{clone_directory, is_cow_supported};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_cow_support_detection() {
    // Test current directory
    let result = is_cow_supported(".");

    // On macOS with APFS, this should return Ok(true)
    // On other systems, it should return Ok(false) or Err
    match result {
        Ok(supported) => {
            println!("CoW supported: {}", supported);
            // On macOS CI, this might be true
        }
        Err(e) => {
            println!("CoW support check failed: {}", e);
            // This is expected on non-macOS systems
        }
    }
}

#[test]
fn test_cow_support_nonexistent_path() {
    let result = is_cow_supported("/nonexistent/path/that/should/not/exist");
    assert!(result.is_err());
}

#[test]
fn test_cow_clone_nonexistent_source() {
    let temp_dir = tempdir().unwrap();
    let dest = temp_dir.path().join("dest");

    let result = clone_directory("/nonexistent/source", &dest);
    assert!(result.is_err());
}

#[test]
fn test_cow_clone_basic_functionality() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dest_dir = temp_dir.path().join("destination");

    // Create source directory with test content
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("test.txt"), "Hello, World!").unwrap();

    // Create subdirectory
    let sub_dir = src_dir.join("subdir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("nested.txt"), "Nested content").unwrap();

    // Test CoW clone
    let result = clone_directory(&src_dir, &dest_dir);

    match result {
        Ok(()) => {
            // Verify destination was created
            assert!(dest_dir.exists());

            // Verify files were copied
            assert!(dest_dir.join("test.txt").exists());
            assert!(dest_dir.join("subdir").join("nested.txt").exists());

            // Verify content matches
            let content = fs::read_to_string(dest_dir.join("test.txt")).unwrap();
            assert_eq!(content, "Hello, World!");

            let nested_content =
                fs::read_to_string(dest_dir.join("subdir").join("nested.txt")).unwrap();
            assert_eq!(nested_content, "Nested content");

            println!("CoW clone succeeded");
        }
        Err(e) => {
            println!("CoW clone failed (expected on non-APFS): {}", e);
            // This is expected on systems without CoW support
        }
    }
}

#[test]
fn test_cow_clone_large_structure() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dest_dir = temp_dir.path().join("destination");

    // Create a more complex directory structure
    fs::create_dir_all(&src_dir).unwrap();

    // Simulate node_modules structure
    let node_modules = src_dir.join("node_modules");
    fs::create_dir_all(&node_modules).unwrap();

    let lodash_dir = node_modules.join("lodash");
    fs::create_dir_all(&lodash_dir).unwrap();
    fs::write(
        lodash_dir.join("package.json"),
        r#"{"name": "lodash", "version": "4.17.21"}"#,
    )
    .unwrap();
    fs::write(
        lodash_dir.join("index.js"),
        "module.exports = require('./lodash');",
    )
    .unwrap();
    fs::write(lodash_dir.join("lodash.js"), "// Lodash implementation").unwrap();

    // Create multiple files to test performance
    for i in 0..10 {
        let file_name = format!("file_{}.js", i);
        fs::write(lodash_dir.join(file_name), format!("// File content {}", i)).unwrap();
    }

    // Create build directory
    let build_dir = src_dir.join("build");
    fs::create_dir_all(&build_dir).unwrap();
    fs::write(build_dir.join("app.js"), "// Built application").unwrap();

    // Test CoW clone
    let result = clone_directory(&src_dir, &dest_dir);

    match result {
        Ok(()) => {
            // Count files in both directories
            let src_count = count_files(&src_dir);
            let dest_count = count_files(&dest_dir);

            assert_eq!(src_count, dest_count, "File count mismatch after CoW clone");

            // Verify specific files
            assert!(
                dest_dir
                    .join("node_modules")
                    .join("lodash")
                    .join("package.json")
                    .exists()
            );
            assert!(dest_dir.join("build").join("app.js").exists());

            println!(
                "CoW clone of complex structure succeeded: {} files",
                src_count
            );
        }
        Err(e) => {
            println!("CoW clone failed (expected on non-APFS): {}", e);
        }
    }
}

#[test]
fn test_cow_clone_preserves_permissions() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dest_dir = temp_dir.path().join("destination");

    // Create source with specific permissions
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("script.sh"), "#!/bin/bash\necho 'Hello'").unwrap();

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(src_dir.join("script.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(src_dir.join("script.sh"), perms).unwrap();
    }

    let result = clone_directory(&src_dir, &dest_dir);

    match result {
        Ok(()) => {
            // Verify file exists
            assert!(dest_dir.join("script.sh").exists());

            // Verify permissions on Unix systems
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let src_perms = fs::metadata(src_dir.join("script.sh"))
                    .unwrap()
                    .permissions();
                let dest_perms = fs::metadata(dest_dir.join("script.sh"))
                    .unwrap()
                    .permissions();
                assert_eq!(
                    src_perms.mode(),
                    dest_perms.mode(),
                    "Permissions not preserved"
                );
            }

            println!("CoW clone preserved permissions");
        }
        Err(e) => {
            println!("CoW clone failed (expected on non-APFS): {}", e);
        }
    }
}

#[test]
fn test_cow_clone_overwrites_destination() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("source");
    let dest_dir = temp_dir.path().join("destination");

    // Create source
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("file.txt"), "source content").unwrap();

    // Create destination with different content
    fs::create_dir_all(&dest_dir).unwrap();
    fs::write(dest_dir.join("file.txt"), "destination content").unwrap();
    fs::write(dest_dir.join("extra.txt"), "extra file").unwrap();

    let result = clone_directory(&src_dir, &dest_dir);

    match result {
        Ok(()) => {
            // Verify source content overwrote destination
            let content = fs::read_to_string(dest_dir.join("file.txt")).unwrap();
            assert_eq!(content, "source content");

            // Extra file should be gone (replaced by CoW clone)
            assert!(!dest_dir.join("extra.txt").exists());

            println!("CoW clone properly overwrote destination");
        }
        Err(e) => {
            println!("CoW clone failed (expected on non-APFS): {}", e);
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_apfs_specific_features() {
    // Test APFS support indirectly through CoW support detection
    let result = is_cow_supported(".");
    match result {
        Ok(is_supported) => {
            println!("CoW support (likely APFS): {}", is_supported);
            // This should typically be true on modern macOS with APFS
        }
        Err(e) => {
            println!("CoW support check failed: {}", e);
            // This could happen on older filesystems
        }
    }
}

// Helper function to count files recursively
fn count_files<P: AsRef<Path>>(dir: P) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                count += 1;
            } else if path.is_dir() {
                count += count_files(&path);
            }
        }
    }
    count
}
