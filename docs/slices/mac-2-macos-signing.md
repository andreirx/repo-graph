# MAC-2: macOS Code Signing and Notarization

Status: DEFERRED
Depends: MAC-1, REL-1
Track: Distribution / Install / Host Integration

## Objective

Implement code signing and notarization for macOS binaries to eliminate Gatekeeper
warnings and enable smooth installation.

## Deferral Rationale

Signing/notarization is deferred because:

1. **Functional first:** Unsigned binaries work — users can allow via Security preferences
   or `xattr` command
2. **Apple Developer account required:** Costs $99/year, requires setup
3. **CI complexity:** Signing in GitHub Actions requires secure credential management
4. **Polish, not function:** This is user experience improvement, not capability

## When to Revisit

Implement MAC-2 when:

1. MAC-1 is shipped and stable
2. User friction from Gatekeeper is significant
3. Apple Developer account is set up
4. Distribution reaches enough users to justify the cost

## Scope (When Implemented)

### Requirements

1. **Apple Developer ID:** Developer ID Application certificate
2. **Notarization:** Submit to Apple for notarization
3. **Stapling:** Attach notarization ticket to binary

### Process

```bash
# 1. Sign binaries
codesign --sign "Developer ID Application: Name (TEAMID)" \
    --options runtime \
    --timestamp \
    rmap

codesign --sign "Developer ID Application: Name (TEAMID)" \
    --options runtime \
    --timestamp \
    rmapd

# 2. Create ZIP for notarization
zip -r rmap-darwin-aarch64.zip rmap rmapd

# 3. Submit for notarization
xcrun notarytool submit rmap-darwin-aarch64.zip \
    --apple-id "email@example.com" \
    --team-id "TEAMID" \
    --password "@keychain:AC_PASSWORD" \
    --wait

# 4. Staple ticket
xcrun stapler staple rmap
xcrun stapler staple rmapd
```

### CI Integration

GitHub Actions workflow additions:

```yaml
- name: Import signing certificate
  env:
    CERTIFICATE_BASE64: ${{ secrets.APPLE_CERTIFICATE_BASE64 }}
    CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
  run: |
    echo "$CERTIFICATE_BASE64" | base64 --decode > certificate.p12
    security create-keychain -p "" build.keychain
    security import certificate.p12 -k build.keychain -P "$CERTIFICATE_PASSWORD" -T /usr/bin/codesign
    security set-key-partition-list -S apple-tool:,apple: -s -k "" build.keychain

- name: Sign binaries
  run: |
    codesign --sign "Developer ID Application: ..." --options runtime --timestamp rmap
    codesign --sign "Developer ID Application: ..." --options runtime --timestamp rmapd

- name: Notarize
  env:
    APPLE_ID: ${{ secrets.APPLE_ID }}
    APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
    APPLE_APP_PASSWORD: ${{ secrets.APPLE_APP_PASSWORD }}
  run: |
    zip -r artifact.zip rmap rmapd
    xcrun notarytool submit artifact.zip --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
    xcrun stapler staple rmap
    xcrun stapler staple rmapd
```

### Secrets Required

| Secret | Description |
|--------|-------------|
| `APPLE_CERTIFICATE_BASE64` | Developer ID certificate (base64) |
| `APPLE_CERTIFICATE_PASSWORD` | Certificate password |
| `APPLE_ID` | Apple ID email |
| `APPLE_TEAM_ID` | Team ID |
| `APPLE_APP_PASSWORD` | App-specific password |

### Verification

```bash
# Verify signature
codesign --verify --verbose=4 rmap

# Verify notarization
spctl --assess --verbose=4 --type execute rmap
```

## Cost

- Apple Developer Program: $99/year

## Alternatives Considered

### Ad-hoc signing

```bash
codesign --sign - rmap
```

Does not help with Gatekeeper — still blocked.

### Homebrew distribution

If repo-graph is distributed via Homebrew, Homebrew handles signing.
But requires Homebrew formula maintenance (separate effort).

## Not in Scope

- Homebrew formula (separate slice)
- DMG installer packaging
- Mac App Store distribution
