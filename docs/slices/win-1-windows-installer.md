# WIN-1: Windows Distribution and Install

Status: DEFERRED
Depends: DIST-1, REL-1
Track: Distribution / Install / Host Integration

## Objective

Implement Windows-specific installer and daemon service for repo-graph.

## Deferral Rationale

Windows is explicitly deprioritized because:

1. **Platform priority decision:** macOS and Linux cover the majority of developer workstations
   in the target user base (AI-assisted coding on real projects)
2. **Complexity:** Windows service management, path handling, and shell differences add
   significant implementation surface
3. **Resource allocation:** Better to ship complete macOS + Linux support first than
   incomplete support across three platforms

## When to Revisit

Consider implementing WIN-1 when:

1. macOS (MAC-1) and Linux (LINUX-1) are stable and shipped
2. User demand demonstrates Windows priority
3. Resources are available for Windows-specific testing

## Scope (When Implemented)

### Platform Specification

- **Target:** Windows 10/11
- **Architecture:** x86_64
- **Service manager:** Windows Service or Task Scheduler
- **Shell:** PowerShell, cmd.exe compatibility

### Directory Layout (Tentative)

```
C:\Program Files\repo-graph\
  rmap.exe
  rmapd.exe

%APPDATA%\repo-graph\
  config.toml
  install-manifest.json

%LOCALAPPDATA%\repo-graph\
  logs\
  databases\
  sessions\
```

### Service Options

1. **Windows Service:** Proper system service (requires admin)
2. **Task Scheduler:** User-level scheduled task (no admin)
3. **Startup folder:** Simple auto-start (least robust)

### Installer Options

1. **PowerShell script:** Similar to bash installer
2. **MSI package:** Standard Windows installer
3. **WinGet manifest:** Package manager integration

### Known Challenges

- Path separators and escaping
- Shell differences (PowerShell vs cmd)
- User vs system installation
- Antivirus interference with unsigned binaries
- Code signing requirements more strict

## Minimal WIN-1 Scope

If implementing with minimal scope:

1. PowerShell install script
2. Task Scheduler for daemon
3. No code signing (user must allow)
4. Claude Code/Codex integration only

## Dependencies

- DIST-1 (contract must allow Windows)
- REL-1 (Windows binary in artifact matrix)
- HOOK-1 (hook commands must work on Windows)

## Not in Scope

- WSL support (use Linux installer)
- Windows ARM64
- Microsoft Store distribution
- Windows Server
