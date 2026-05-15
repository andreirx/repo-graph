# UPDATE-1: Updater and Repair Channel

Status: DEFERRED
Depends: DIST-1, MAC-1, LINUX-1
Track: Distribution / Install / Host Integration

## Objective

Implement auto-update and repair capabilities for repo-graph installations.

## Deferral Rationale

Auto-update is deferred because:

1. **Complexity:** Auto-update adds significant system complexity:
   - Release channel management
   - Rollback policy
   - Service replacement safety
   - Update notification UX
   - Signed artifact verification
2. **Trust surface:** Auto-update is a privileged operation that modifies system binaries
3. **Manual update works:** Users can re-run installer for updates
4. **Field experience needed:** Need to understand real-world update patterns first

## When to Revisit

Implement UPDATE-1 when:

1. MAC-1 and LINUX-1 are stable
2. Release cadence is established
3. User base is large enough to justify investment
4. Security model for updates is designed

## Scope (When Implemented)

### Update Check

```
$ rmap update check

Current version: 0.1.0
Latest version: 0.2.0

Changes in 0.2.0:
  - New feature X
  - Bug fix Y

Run 'rmap update apply' to update.
```

### Update Apply

```
$ rmap update apply

Downloading rmap 0.2.0...
Verifying checksum...
Stopping daemon...
Installing new binaries...
Starting daemon...
Verifying health...

Update complete: 0.1.0 → 0.2.0
```

### Release Channels

| Channel | Description | Update Policy |
|---------|-------------|---------------|
| `stable` | Production releases | Manual or prompted |
| `beta` | Pre-release testing | Opt-in |
| `nightly` | Daily builds | Opt-in, no stability guarantee |

### Configuration

```toml
# ~/.config/rmap/config.toml
[update]
channel = "stable"
check_interval = "daily"
auto_apply = false  # Never auto-apply without consent
notify = true       # Show notification when update available
```

### Rollback

```
$ rmap update rollback

Previous version: 0.1.0
Rolling back...

Rollback complete: 0.2.0 → 0.1.0
```

Requires keeping previous version binaries.

### Repair

```
$ rmap repair

Checking installation...
  ✓ Binaries present
  ✓ Directories exist
  ✗ Daemon service not registered

Repairing...
  Registering daemon service...
  Starting daemon...

Repair complete.
```

## Security Considerations

### Signed Updates

Updates must be signed and verified:

1. Release artifacts signed with repo-graph release key
2. Update client verifies signature before applying
3. Signature verification is mandatory, not optional

### Update Transport

- HTTPS only
- Certificate pinning (optional, adds complexity)
- Checksum verification

### Privilege Escalation

Update may require elevated privileges:
- macOS: may need sudo for /usr/local/bin
- Linux: may need sudo for /usr/local/bin

Design for minimal privilege:
- User-local install doesn't need sudo
- System-wide install prompts for sudo only when needed

## Implementation Complexity

### Atomic Updates

Binary replacement must be atomic:
1. Download new binary to temp location
2. Verify checksum and signature
3. Stop daemon
4. Rename old binary to backup
5. Move new binary to final location
6. Start daemon
7. Verify health
8. If failure, restore backup

### Platform Differences

| Aspect | macOS | Linux |
|--------|-------|-------|
| Service restart | launchctl | systemctl |
| Binary location | /usr/local/bin | /usr/local/bin |
| Privilege | sudo if needed | sudo if needed |

## Alternative: Manual Update

Until UPDATE-1, users update manually:

```bash
# Re-run installer
curl -fsSL https://github.com/{OWNER}/repo-graph/releases/latest/download/install.sh | bash
```

Installer handles:
- Version detection
- Daemon restart
- Configuration preservation

## Not in Scope

- Delta updates (full binary replacement only)
- Background update downloads
- Update scheduling
- Enterprise update policies
- Air-gapped update mechanisms
