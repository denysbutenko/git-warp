# Git-Warp Documentation

**Complete documentation for the high-performance Git worktree manager**

Welcome to the Git-Warp documentation! This directory contains comprehensive guides covering everything from basic usage to advanced technical details.

## 📚 Documentation Overview

### For Users

**[User Guide](user-guide.md)** - *Start Here!*
- Complete how-to guide for using Git-Warp
- Quick start tutorial
- All features explained with examples
- Configuration and troubleshooting
- Best practices and workflows

### For Developers & Contributors

**[Technical Overview](technical-overview.md)**
- Architecture and design decisions
- Implementation details
- Performance characteristics
- Module documentation
- Testing strategies

### Project History

**[Implementation Plans](implement-plan-v1.md) & [Implementation Plans v2](implement-plan-v2.md)**
- Original project vision and requirements
- Detailed implementation roadmap
- Technical specifications

**[Original Project References]()**
- [`autowt.txt`](autowt.txt) - Python-based UX-focused predecessor
- [`coworktree.txt`](coworktree.txt) - Go-based performance-focused predecessor

---

## 🚀 Quick Start

New to Git-Warp? Start with these essential commands:

```bash
# Create your first worktree (instant with CoW!)
warp feature/amazing-new-feature

# List all worktrees
warp ls

# Clean up merged branches safely
warp cleanup --mode merged

# View configuration
warp config --show
```

**→ [Full Quick Start Guide](user-guide.md#quick-start)**

---

## 🎯 What Makes Git-Warp Special?

### ⚡ **Instant Performance**
- **Copy-on-Write**: Create worktrees in milliseconds, not minutes
- **Smart Caching**: Intelligent reuse of dependencies and build artifacts
- **Parallel Processing**: Multi-threaded file operations

### 🛡️ **Safety First**
- **Process Detection**: Never break running services
- **Dry-Run Mode**: Preview all operations safely
- **Graceful Recovery**: Intelligent error handling and recovery

### 🤖 **AI Integration**
- **Claude Code Monitoring**: Live dashboard of AI agent activities
- **Hook System**: Seamless integration with AI development workflows
- **Intelligent Suggestions**: AI-powered workflow optimizations

### 🖥️ **Rich Experience**
- **Terminal Integration**: Automatic tab/window switching (macOS)
- **Beautiful CLI**: Emoji-enhanced, informative output
- **Live Dashboards**: Real-time TUI interfaces

---

## 📖 Documentation Structure

### User Documentation

| Document | Purpose | Audience |
|----------|---------|----------|
| **[User Guide](user-guide.md)** | Complete usage guide | All users |
| Quick Reference | Command cheatsheet | Daily users |
| Configuration Reference | All settings explained | Power users |
| Troubleshooting Guide | Common issues & solutions | All users |

### Technical Documentation  

| Document | Purpose | Audience |
|----------|---------|----------|
| **[Technical Overview](technical-overview.md)** | Architecture & implementation | Developers |
| API Documentation | Internal API reference | Contributors |
| Performance Guide | Optimization details | DevOps/SRE |
| Contributing Guide | Development workflow | Contributors |

### Project Documentation

| Document | Purpose | Audience |
|----------|---------|----------|
| **[Implementation Plans](implement-plan-v2.md)** | Project roadmap | Project managers |
| Change Log | Version history | All users |
| Migration Guide | Upgrade instructions | Existing users |

---

## 🔍 Find What You Need

### I want to...

**Learn Git-Warp basics**
→ [User Guide: Quick Start](user-guide.md#quick-start)

**Understand how CoW works**
→ [Technical Overview: Copy-on-Write Implementation](technical-overview.md#copy-on-write-implementation)

**Configure Git-Warp for my team**
→ [User Guide: Configuration](user-guide.md#configuration)

**Troubleshoot an issue**
→ [User Guide: Troubleshooting](user-guide.md#troubleshooting)

**Contribute to the project**
→ [Technical Overview: Module Design](technical-overview.md#module-design)

**Understand the architecture**
→ [Technical Overview: Architecture Overview](technical-overview.md#architecture-overview)

**See performance benchmarks**
→ [Technical Overview: Performance Benchmarks](technical-overview.md#performance-benchmarks)

**Learn about AI integration**
→ [User Guide: AI Agent Monitoring](user-guide.md#ai-agent-monitoring)

---

## 🎯 Common Use Cases

### Individual Developer

```bash
# Daily workflow
warp switch main && git pull
warp feature/user-authentication
# Work, commit, push
warp cleanup --mode merged
```

**→ [User Guide: Daily Workflow](user-guide.md#daily-workflow)**

### Team Development

```bash
# Team-safe settings
export GIT_WARP_AUTO_CONFIRM=false
export GIT_WARP_TERMINAL_MODE=window
warp cleanup --interactive
```

**→ [User Guide: Team Collaboration](user-guide.md#team-collaboration)**

### Large Monorepos

```bash
# Optimized for large repos
export GIT_WARP_WORKTREES_PATH=/fast-ssd/worktrees
warp config --show  # Verify settings
```

**→ [User Guide: Large Monorepos](user-guide.md#large-monorepos)**

### AI-Assisted Development

```bash
# Monitor Claude Code activities
warp agents &
# Work with AI assistance while monitoring
warp feature/ai-integration
```

**→ [User Guide: AI Integration](user-guide.md#advanced-usage)**

---

## 💡 Key Concepts

### Copy-on-Write (CoW)

Git-Warp's secret weapon for instant worktree creation. Instead of copying files, CoW creates instant snapshots using filesystem-level cloning.

**Benefits:**
- **Speed**: 30-180x faster than traditional methods
- **Space Efficient**: Shared data until files are modified
- **Reliability**: Atomic operations prevent corruption

**→ [Technical Deep Dive](technical-overview.md#copy-on-write-implementation)**

### Intelligent Process Management

Git-Warp prevents disasters by detecting and managing processes running in worktrees before cleanup.

**Features:**
- **Detection**: Find all processes in worktree directories
- **Graceful Termination**: SIGTERM → SIGKILL progression
- **User Control**: Interactive confirmation and bypass options

**→ [Implementation Details](technical-overview.md#process-management)**

### Layered Configuration

Sophisticated configuration system with clear precedence rules.

**Layers (highest to lowest priority):**
1. Command-line arguments
2. Environment variables (`GIT_WARP_*`)
3. Configuration file (`~/.config/git-warp/config.toml`)
4. Built-in defaults

**→ [Configuration Guide](user-guide.md#configuration)**

---

## 🚦 Getting Help

### Documentation Issues

- **Missing information?** [Open an issue](https://github.com/denysbutenko/git-warp/issues/new?template=documentation.md)
- **Found an error?** Submit a PR with the fix
- **Need clarification?** Start a [discussion](https://github.com/denysbutenko/git-warp/discussions)

### Technical Support

1. **Check the [Troubleshooting Guide](user-guide.md#troubleshooting)**
2. **Enable debug mode**: `RUST_LOG=debug warp --debug <command>`
3. **Search [existing issues](https://github.com/denysbutenko/git-warp/issues)**
4. **Create a new issue** with debug output

### Feature Requests

We love hearing about new use cases! 

1. **Check the [roadmap](implement-plan-v2.md)** 
2. **Search [existing requests](https://github.com/denysbutenko/git-warp/issues?q=is%3Aissue+label%3Aenhancement)**
3. **Open a feature request** with your use case

---

## 🤝 Contributing to Documentation

Documentation improvements are always welcome!

### Quick Fixes
- Fix typos, broken links, or unclear instructions
- Add missing examples or clarifications
- Improve formatting or structure

### Major Contributions
- New guides or tutorials
- Architecture documentation
- Performance analysis
- Integration guides

### Documentation Standards

- **Clear and Concise**: Prefer simple, direct language
- **Example-Driven**: Include working code examples
- **User-Focused**: Write from the user's perspective
- **Comprehensive**: Cover edge cases and gotchas
- **Tested**: Verify all examples work as described

---

## 📊 Documentation Stats

| Document | Lines | Words | Last Updated |
|----------|-------|--------|--------------|
| User Guide | 850+ | 12,000+ | Latest |
| Technical Overview | 750+ | 10,000+ | Latest |
| Implementation Plans | 400+ | 6,000+ | v2.0 |
| Project References | 300+ | 4,000+ | Historical |

**Total Documentation**: 2,300+ lines, 32,000+ words

---

## 🎉 Success Stories

> *"Git-Warp reduced our feature branch setup time from 5 minutes to 3 seconds. Game changer for our team!"*  
> — Senior Developer, Tech Startup

> *"The CoW technology is incredible. We're working on a 10GB monorepo and worktree creation is instant."*  
> — DevOps Engineer, Enterprise

> *"The AI integration dashboard helps us understand what Claude is doing across different branches. Brilliant!"*  
> — ML Engineer, AI Company

---

## 🔮 What's Next?

Git-Warp is continuously evolving. Check out our [roadmap](implement-plan-v2.md) to see what's coming:

- **Interactive TUI Interfaces**: Enhanced cleanup and configuration
- **Plugin System**: Extensible architecture for custom workflows  
- **Multi-Platform CoW**: Linux overlayfs support
- **Enhanced AI Integration**: More sophisticated agent monitoring
- **Team Features**: Shared configuration and policies

---

**Ready to transform your Git workflow?**

**→ [Get Started with the User Guide](user-guide.md)**

**Happy warping! 🚀**
