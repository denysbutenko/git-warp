# Git-Warp User Guide

**The Ultimate Git Worktree Manager: Lightning-Fast, AI-Integrated, and Developer-Friendly**

## Table of Contents

1. [What is Git-Warp?](#what-is-git-warp)
2. [Why Use Git-Warp?](#why-use-git-warp)
3. [Installation](#installation)
4. [Quick Start](#quick-start)
5. [Core Features](#core-features)
6. [Advanced Usage](#advanced-usage)
7. [Configuration](#configuration)
8. [Troubleshooting](#troubleshooting)
9. [Best Practices](#best-practices)

---

## What is Git-Warp?

Git-Warp is a high-performance Git worktree manager that revolutionizes how developers work with multiple branches simultaneously. Built in Rust for maximum speed and reliability, it combines:

- **⚡ Instant Worktree Creation**: Copy-on-Write (CoW) technology creates worktrees in milliseconds
- **🤖 AI Integration**: Live monitoring of Claude Code agent activities
- **🛡️ Process Safety**: Intelligent process detection and management
- **🖥️ Terminal Integration**: Seamless switching between worktrees in your favorite terminal
- **⚙️ Rich Configuration**: Powerful, layered configuration system

### The Problem Git-Warp Solves

Traditional Git workflows force developers to choose between:
- **Stashing/Committing**: Interrupting current work to switch branches
- **Multiple Repositories**: Managing separate clones (disk space, sync issues)
- **Manual Worktrees**: Complex `git worktree` commands with cleanup headaches

Git-Warp eliminates these compromises by making worktrees effortless, instant, and intelligent.

---

## Why Use Git-Warp?

### 🚀 **Speed That Changes Everything**

```bash
# Traditional approach (30+ seconds)
git stash
git checkout feature-branch
npm install
# Work on feature
git checkout main
git stash pop

# Git-Warp approach (< 1 second)
warp feature-branch
# Instantly ready to work - dependencies already installed!
```

### 🧠 **Intelligence Built-In**

- **Process Detection**: Never accidentally break running services
- **Branch Analysis**: Automatic detection of merged/stale branches
- **Agent Monitoring**: See what Claude Code is doing in real-time
- **Smart Cleanup**: AI-powered suggestions for worktree maintenance

### 💎 **Developer Experience**

- **Zero Configuration**: Works perfectly out-of-the-box
- **Rich Feedback**: Beautiful, informative CLI output
- **Error Prevention**: Dry-run mode for all destructive operations
- **Terminal Magic**: Automatic tab/window switching

---

## Installation

### Prerequisites

- **Rust**: 2024 edition (latest stable)
- **Git**: Any modern version
- **macOS**: Required for Copy-on-Write support (APFS filesystem)

### Build from Source

```bash
# Clone the repository
git clone https://github.com/denysbutenko/git-warp
cd git-warp

# Build optimized binary
cargo build --release

# The binary will be at target/release/warp
# Optionally, add to your PATH
sudo ln -sf $(pwd)/target/release/warp /usr/local/bin/warp
```

### Verify Installation

```bash
warp --version
# Should output: warp 0.1.0
```

---

## Quick Start

### Your First Worktree

```bash
# Navigate to any Git repository
cd your-project

# Create and switch to a new feature branch (instant!)
warp feature/amazing-new-feature

# List all worktrees
warp ls

# Work normally - everything is ready to go!
```

### Essential Commands

```bash
# Switch to existing branch
warp switch existing-branch

# Switch to the most recent or waiting agent branch
warp switch --latest
warp switch --waiting

# List worktrees with details
warp ls --debug

# Clean up merged branches
warp cleanup --mode merged

# View configuration
warp config --show

# Monitor AI agents (if using Claude Code)
warp agents
```

---

## Core Features

### 1. Instant Worktree Creation

Git-Warp uses Copy-on-Write technology to create worktrees instantly:

```bash
# Creates worktree in milliseconds vs. minutes
warp feature/user-authentication

# Custom path
warp switch feature/ui-redesign --path /custom/path

# Force traditional method (skip CoW)
warp switch testing-branch --no-cow
```

**How it Works**: On APFS filesystems (macOS), Git-Warp clones your repository's file tree instantly using CoW, then intelligently rewrites absolute paths in configuration files.

### 2. Intelligent Worktree Management

**List Worktrees**:
```bash
warp ls
# Output:
# 📁 Git Worktrees:
# 
# 🏠  main /Users/you/project
# 🌿  feature-branch /Users/you/project/../worktrees/feature-branch
# 🌿  hotfix-123 /Users/you/project/../worktrees/hotfix-123
# 
# 📊 Total: 3 worktrees
```

**Advanced Listing**:
```bash
warp ls --debug
# Shows HEAD commits, branch status, and more details
```

### 3. Smart Cleanup

Git-Warp analyzes your branches intelligently:

```bash
# Clean up merged branches
warp cleanup --mode merged

# Clean up branches without remotes
warp cleanup --mode remoteless

# Interactive cleanup (TUI interface)
warp cleanup --interactive

# Dry-run to see what would be cleaned
warp --dry-run cleanup --mode all
```

**Cleanup Modes**:
- `merged`: Branches merged into main/master
- `remoteless`: Branches without remote tracking
- `all`: All non-main worktrees (use with caution!)
- `interactive`: Choose which worktrees to remove

### 4. Process Safety

Git-Warp prevents disasters by detecting running processes:

```bash
# Automatically detects processes in worktrees
warp cleanup --mode merged

# If processes are found:
# ⚠️  Found 2 processes in worktree
#   • PID 1234: npm (CPU: 15.2%, Mem: 45MB)
#     Working dir: /project/worktrees/feature-branch
#   • PID 5678: webpack-dev-server (CPU: 8.1%, Mem: 120MB)
#     Command: node webpack serve --mode development

# Kill processes automatically
warp cleanup --mode merged --kill

# Force cleanup ignoring processes
warp cleanup --mode merged --force
```

### 5. Terminal Integration

Seamless integration with macOS terminals:

```bash
# Open new tab (default)
warp switch feature-branch

# Open new window
warp switch --terminal window feature-branch

# Echo commands instead of switching
warp switch --terminal echo feature-branch

# Stay in current location
warp switch --terminal inplace feature-branch
```

### 6. AI Agent Monitoring

Real-time monitoring of Claude Code activities:

```bash
# Launch live dashboard
warp agents
```

**Dashboard Features**:
- Live agent activity feed
- CPU and memory usage
- Activity statistics
- Interactive navigation (↑↓ keys, r to refresh)

---

## Advanced Usage

### Configuration Management

**View Current Settings**:
```bash
warp config --show
# Shows all configuration with current values
```

**Open the Config File**:
```bash
warp config --edit
# Creates the default config if needed, then opens it in your editor
```

**Environment Variables** (Override any setting):
```bash
export GIT_WARP_TERMINAL_MODE=window
export GIT_WARP_USE_COW=true
export GIT_WARP_AUTO_CONFIRM=false
export GIT_WARP_WORKTREES_PATH=/custom/worktrees
```

### Dry-Run Mode

Preview all operations safely:

```bash
# See what would be done
warp --dry-run switch feature-branch
warp --dry-run cleanup --mode all
warp --dry-run --terminal window switch testing
```

### Branch Naming & Patterns

Git-Warp handles complex branch names gracefully:

```bash
# Forward slashes (creates nested structure)
warp feature/user-auth/login-form

# Special characters
warp "hotfix/issue-#123"

# Automatic sanitization for filesystem paths
warp "feature branch with spaces"  # → feature-branch-with-spaces
```

### Custom Worktree Paths

```bash
# Specific path
warp switch mybranch --path /tmp/quick-test

# Pattern-based paths (configured)
# ~/worktrees/project-name/branch-name
```

### Integration with Development Workflow

**With Package Managers**:
```bash
# Dependencies are already installed via CoW!
warp feature/new-ui
cd ../worktrees/feature-new-ui
npm start  # Works immediately
```

**With Claude Code**:
```bash
# Monitor AI agent activity while developing
warp agents &  # Background monitoring
warp switch ai-integration
# Work with Claude Code while monitoring agent activities
```

**With Docker/Services**:
```bash
# Safe service management
warp cleanup --kill --mode merged  # Stops services in cleaned worktrees
warp switch main
docker-compose up  # Start services in main worktree
```

---

## Configuration

Git-Warp uses a sophisticated configuration system with three layers:

### 1. Configuration File

Located at: `~/.config/git-warp/config.toml`

```toml
# Terminal mode: tab, window, inplace, echo
terminal_mode = "tab"

# Use Copy-on-Write when available
use_cow = true

# Auto-confirm destructive operations
auto_confirm = false

# Custom worktrees directory (optional)
# worktrees_path = "/custom/path/to/worktrees"

[git]
# Default main branch name
default_branch = "main"

# Auto-fetch before operations
auto_fetch = true

# Auto-prune remote tracking branches
auto_prune = true

[process]
# Check for processes before cleanup
check_processes = true

# Auto-kill processes during cleanup
auto_kill = false

# Grace period before force killing (seconds)
kill_timeout = 5

[terminal]
# Terminal app: auto, iterm2, terminal
app = "auto"

# Auto-activate new tabs/windows
auto_activate = true

[agent]
# Enable agent monitoring
enabled = true

# Refresh rate for agent dashboard (milliseconds)
refresh_rate = 1000

# Maximum activities to track
max_activities = 100

# Enable Claude Code hooks integration
claude_hooks = true
```

### 2. Environment Variables

Override any setting with `GIT_WARP_` prefix:

```bash
# Terminal behavior
export GIT_WARP_TERMINAL_MODE=window
export GIT_WARP_AUTO_CONFIRM=true

# Performance
export GIT_WARP_USE_COW=false  # Disable CoW

# Paths
export GIT_WARP_WORKTREES_PATH=/Users/me/dev/worktrees
```

### 3. Command-Line Options

Highest priority, overrides everything:

```bash
warp --terminal window --auto-confirm switch feature-branch
```

### Configuration Priority

1. **Command-line options** (highest)
2. **Environment variables** 
3. **Configuration file**
4. **Built-in defaults** (lowest)

---

## Troubleshooting

### Common Issues

**CoW Not Working**:
```bash
# Check filesystem type
df -T .
# Should show "apfs" for CoW support

# Force traditional method
warp switch --no-cow branch-name
```

**Terminal Integration Issues**:
```bash
# Check terminal support
warp config --show
# Look for "Terminal Integration" section

# Try different terminal mode
warp --terminal echo switch branch-name
```

**Permission Errors**:
```bash
# Check directory permissions
ls -la ../worktrees/

# Create worktrees directory manually
mkdir -p ../worktrees
```

**Process Detection Issues**:
```bash
# Bypass process checks
warp cleanup --force --mode merged

# Check what processes are detected
warp --dry-run cleanup --mode merged
```

### Debug Mode

Enable verbose logging:

```bash
RUST_LOG=debug warp --debug switch feature-branch
```

### Recovery Operations

**Clean Up Corrupted Worktrees**:
```bash
# Remove broken worktree references
git worktree prune

# List remaining worktrees
git worktree list

# Manual removal if needed
rm -rf ../worktrees/broken-branch
git worktree remove --force broken-branch
```

**Reset Configuration**:
```bash
# Remove config file
rm ~/.config/git-warp/config.toml

# Recreate and open the default config
warp config --edit
```

---

## Best Practices

### 1. Worktree Organization

**Recommended Structure**:
```
your-project/
├── .git/
├── main-branch-files/
└── ../worktrees/
    ├── feature-authentication/
    ├── hotfix-security-issue/
    └── experimental-ui/
```

**Naming Conventions**:
- Use descriptive, kebab-case names: `feature-user-auth`
- Include type prefix: `feature/`, `hotfix/`, `experiment/`
- Keep names short but clear

### 2. Development Workflow

**Daily Workflow**:
```bash
# Start of day
warp switch main
git pull

# New feature
warp feature/amazing-feature
# Work, commit, push

# Code review
warp switch main
# Review PRs

# Quick hotfix
warp hotfix/critical-bug
# Fix, test, deploy

# End of day cleanup
warp cleanup --mode merged
```

**Team Collaboration**:
- Share branch naming conventions
- Use consistent worktree organization
- Document custom configuration settings

### 3. Performance Optimization

**Maximize CoW Benefits**:
- Keep `node_modules/` and build artifacts in main worktree
- Use `.gitignore` to exclude large generated files
- Regularly clean up old worktrees

**Resource Management**:
- Monitor disk usage: `du -sh ../worktrees/*`
- Set reasonable limits in configuration
- Use cleanup commands regularly

### 4. Safety Practices

**Before Destructive Operations**:
```bash
# Always dry-run first
warp --dry-run cleanup --mode all

# Check for uncommitted changes
warp ls --debug

# Backup important work
git stash --include-untracked  # In each worktree
```

**Process Management**:
- Always check for running processes
- Use graceful termination (avoid --force unless necessary)
- Monitor services that might span worktrees

### 5. Integration Tips

**With IDEs**:
- Configure IDE to handle multiple project roots
- Use workspace features for multi-worktree development
- Set up consistent formatting/linting across worktrees

**With CI/CD**:
- Ensure CI systems understand worktree structure
- Use relative paths in configuration files
- Test deployment from different worktrees

**With Docker**:
- Be careful with volume mounts spanning worktrees
- Consider worktree-specific docker-compose files
- Clean up containers when removing worktrees

---

## Advanced Scenarios

### Large Monorepos

```bash
# Set custom worktree location to faster disk
export GIT_WARP_WORKTREES_PATH=/fast-ssd/worktrees

# Increase cleanup thresholds
warp config --edit
# Adjust max_activities and other limits
```

### Multi-Team Development

```bash
# Team-specific configuration
export GIT_WARP_TERMINAL_MODE=window  # For team preference
export GIT_WARP_AUTO_CONFIRM=false    # Safety for shared environments

# Standardize worktree structure
export GIT_WARP_WORKTREES_PATH=/teams/shared/worktrees
```

### Continuous Integration

```bash
# CI-friendly settings
export GIT_WARP_AUTO_CONFIRM=true
export GIT_WARP_USE_COW=false  # May not be available in CI
export GIT_WARP_TERMINAL_MODE=echo
```

---

Git-Warp transforms Git worktree management from a complex, error-prone process into an effortless, intelligent workflow. Whether you're working on small features or managing large monorepos, Git-Warp's combination of speed, safety, and intelligence makes it an indispensable tool for modern development.

**Happy warping! 🚀**
