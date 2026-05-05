# BI-EM-1: Inter-Core Messaging Detection (Mailbox / RPMsg)

**Status:** PLANNED  
**Slice:** BI-EM-1  
**Family:** Embedded / Inter-Core  
**Language:** C (first)

## Problem

Heterogeneous multi-core SoCs (MPU + MCU, Linux + RTOS) communicate via
inter-core mechanisms: hardware mailboxes and RPMsg (Remote Processor Messaging).
These are critical boundary surfaces in embedded Linux systems.

Agents working on SoC firmware need to see where inter-core communication
exists. This is discovery-oriented: we surface the presence and location of
inter-core IPC, not runtime message flow or buffer state.

## Background

### Mailbox

Hardware mailbox controllers provide low-level signaling between cores.
Linux exposes these via the mailbox framework (`drivers/mailbox/`).

Key APIs:
- `mbox_request_channel()` / `mbox_free_channel()` — channel lifecycle
- `mbox_send_message()` — send doorbell/message
- Receive via callback registered at channel request

### RPMsg

RPMsg (Remote Processor Messaging) is a higher-level protocol built on
virtio/shared memory. It provides named endpoints for message exchange.

Key APIs:
- `rpmsg_create_ept()` / `rpmsg_destroy_ept()` — endpoint lifecycle
- `rpmsg_send()` / `rpmsg_trysend()` — send message
- `rpmsg_recv()` — receive (or callback-based)
- `rpmsg_register_device()` — device registration

### Remoteproc (Excluded)

Remoteproc lifecycle APIs (`rproc_boot`, `rproc_shutdown`, `rproc_alloc`, etc.)
are control-plane operations that start/stop remote processors. They are not
message exchange surfaces and are excluded from this slice.

**Rationale:** Lifecycle management is fundamentally different from data-plane
messaging. `rproc_boot` does not send a message to another software entity;
it starts a processor. Including lifecycle operations would conflate control-plane
with data-plane semantics.

## Scope

### Mailbox Framework

| Function | Role | Notes |
|----------|------|-------|
| `mbox_request_channel` | setup | Request mailbox channel |
| `mbox_request_channel_byname` | setup | Request by name |
| `mbox_free_channel` | teardown | Release channel |
| `mbox_send_message` | provider | Send message/doorbell |
| `mbox_client_txdone` | provider | TX completion notification |
| `mbox_client_peek_data` | consumer | Check for pending data |

### RPMsg

| Function | Role | Notes |
|----------|------|-------|
| `rpmsg_create_ept` | setup | Create endpoint |
| `rpmsg_destroy_ept` | teardown | Destroy endpoint |
| `rpmsg_send` | provider | Send message |
| `rpmsg_sendto` | provider | Send to specific address |
| `rpmsg_send_offchannel` | provider | Send via specific src/dst |
| `rpmsg_trysend` | provider | Non-blocking send |
| `rpmsg_trysendto` | provider | Non-blocking send to address |
| `rpmsg_recv` | consumer | Receive message (if not callback) |
| `rpmsg_register_device` | setup | Register RPMsg device |

**In scope:**
- C syntax-level detection of mailbox/RPMsg messaging APIs
- Evidence extraction (channel name, endpoint name where visible)
- Boundary surface emission per callsite

**Out of scope:**
- Remoteproc lifecycle APIs (control-plane, not data-plane)
- Shared memory buffer analysis
- Message content/protocol inference
- Remote processor identity correlation
- Virtio ring analysis
- Runtime channel state

## Surface Semantics

### Channel Kind

**Decision:** Create new `inter_core_channel` channel kind.

**Rationale:**
- Neutral name covering both mailbox and RPMsg mechanisms
- Fundamentally different from local IPC (crosses processor boundaries)
- Distinct hardware substrate (mailbox controllers, shared SRAM)
- Separate from network/socket patterns
- Enables targeted queries for embedded firmware analysis

### Surface Properties

| Property | Value |
|----------|-------|
| `channel_kind` | `inter_core_channel` |
| `api_family` | `mailbox` or `rpmsg` |
| `protocol_family` | `inter_core` |
| `boundary_scope` | `unknown` |
| `interaction_pattern` | `message_passing` or `fire_and_forget` |

### Boundary Scope

**Decision:** Use `unknown` for now.

**Rationale:** Same-SoC inter-core links (MPU + MCU on one chip) do not fit
cleanly into existing scope vocabulary:
- `inter_process` implies shared OS, which may not exist
- `inter_device` implies separate physical devices, which is inaccurate

Rather than assert an incorrect scope or introduce a new concept prematurely,
we mark scope as `unknown`. This can be refined when inter-core messaging
proves useful enough to justify schema expansion.

### Direction Semantics

| Function | Direction | Rationale |
|----------|-----------|-----------|
| `mbox_request_channel*` | `bidirectional` | Setup; role unclear |
| `mbox_free_channel` | `bidirectional` | Teardown |
| `mbox_send_message` | `provider` | Sends to remote |
| `mbox_client_txdone` | `provider` | TX completion |
| `mbox_client_peek_data` | `consumer` | Check for inbound data |
| `rpmsg_create_ept` | `bidirectional` | Creates endpoint |
| `rpmsg_destroy_ept` | `bidirectional` | Destroys endpoint |
| `rpmsg_send*` | `provider` | Sends message |
| `rpmsg_recv` | `consumer` | Receives message |
| `rpmsg_register_device` | `bidirectional` | Device registration |

## Evidence Payload

```json
{
  "function": "rpmsg_create_ept",
  "api_family": "rpmsg",
  "endpoint_name": "rpmsg-sensor",  // if literal extractable
  "basis": "api_call"
}
```

For mailbox:
```json
{
  "function": "mbox_send_message",
  "api_family": "mailbox",
  "basis": "api_call"
}
```

## Known Limits

1. **Callback-based receive:** RPMsg and mailbox often use callbacks for
   receive. The callback registration is indirect and may not be detected.

2. **Device tree configuration:** Mailbox channels are often configured in
   device tree, not C code. Channel identity may not be visible in source.

3. **Wrapper layers:** SoC vendors often wrap these APIs. Detection may
   miss vendor-specific wrappers without additional bindings.

4. **No shared memory analysis:** The underlying shared memory regions
   used by RPMsg/virtio are not analyzed in this slice.

5. **Kernel-only:** These are kernel APIs. Userspace RPMsg access (via
   /dev/rpmsg*) would require different detection.

6. **Scope ambiguity:** `boundary_scope = unknown` because inter-core
   does not map cleanly to existing scope vocabulary.

## Implementation

### Phase 1: Add Channel Kind and Protocol Family

Add to `rust/crates/boundary-interaction/src/types.rs`:
- `ChannelKind::InterCoreChannel`
- `ProtocolFamily::InterCore`

### Phase 2: Binding Table

Add to `rust/crates/boundary-interaction/bindings.toml`:

```toml
# ════════════════════════════════════════════════════════════════════════
# MAILBOX FRAMEWORK (BI-EM-1)
# ════════════════════════════════════════════════════════════════════════

[[binding]]
language = "c"
api_family = "mailbox"
function = "mbox_request_channel"
role = "bidirectional"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Request mailbox channel from controller."

[[binding]]
language = "c"
api_family = "mailbox"
function = "mbox_request_channel_byname"
role = "bidirectional"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
arg_index = 1
notes = "Request mailbox channel by name. Arg1 is name."

[[binding]]
language = "c"
api_family = "mailbox"
function = "mbox_free_channel"
role = "bidirectional"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Release mailbox channel."

[[binding]]
language = "c"
api_family = "mailbox"
function = "mbox_send_message"
role = "provider"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "fire_and_forget"
basis = "api_call"
notes = "Send message/doorbell to remote core."

[[binding]]
language = "c"
api_family = "mailbox"
function = "mbox_client_txdone"
role = "provider"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "fire_and_forget"
basis = "api_call"
notes = "TX completion notification."

[[binding]]
language = "c"
api_family = "mailbox"
function = "mbox_client_peek_data"
role = "consumer"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Check for pending inbound data."

# ════════════════════════════════════════════════════════════════════════
# RPMSG (BI-EM-1)
# ════════════════════════════════════════════════════════════════════════

[[binding]]
language = "c"
api_family = "rpmsg"
function = "rpmsg_create_ept"
role = "bidirectional"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Create RPMsg endpoint."

[[binding]]
language = "c"
api_family = "rpmsg"
function = "rpmsg_destroy_ept"
role = "bidirectional"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Destroy RPMsg endpoint."

[[binding]]
language = "c"
api_family = "rpmsg"
function = "rpmsg_send"
role = "provider"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Send message via RPMsg."

[[binding]]
language = "c"
api_family = "rpmsg"
function = "rpmsg_sendto"
role = "provider"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Send message to specific RPMsg address."

[[binding]]
language = "c"
api_family = "rpmsg"
function = "rpmsg_send_offchannel"
role = "provider"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Send via specific src/dst."

[[binding]]
language = "c"
api_family = "rpmsg"
function = "rpmsg_trysend"
role = "provider"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Non-blocking RPMsg send."

[[binding]]
language = "c"
api_family = "rpmsg"
function = "rpmsg_trysendto"
role = "provider"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Non-blocking send to address."

[[binding]]
language = "c"
api_family = "rpmsg"
function = "rpmsg_recv"
role = "consumer"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Receive message via RPMsg."

[[binding]]
language = "c"
api_family = "rpmsg"
function = "rpmsg_register_device"
role = "bidirectional"
channel_kind = "inter_core_channel"
scope_heuristic = "fixed"
fixed_scope = "unknown"
interaction_pattern = "message_passing"
basis = "api_call"
notes = "Register RPMsg device with subsystem."
```

### Phase 3: Extractor Support

Add to `rust/crates/c-extractor/src/boundary_detector.rs`:

```rust
const MAILBOX_FUNCTIONS: &[&str] = &[
    "mbox_request_channel", "mbox_request_channel_byname",
    "mbox_free_channel", "mbox_send_message",
    "mbox_client_txdone", "mbox_client_peek_data",
];
const RPMSG_FUNCTIONS: &[&str] = &[
    "rpmsg_create_ept", "rpmsg_destroy_ept",
    "rpmsg_send", "rpmsg_sendto", "rpmsg_send_offchannel",
    "rpmsg_trysend", "rpmsg_trysendto",
    "rpmsg_recv", "rpmsg_register_device",
];
```

### Phase 4: Tests

Create fixtures with kernel-like patterns (header stubs + usage).

## Validation Plan

### Validation Corpus

Per corpus guidance, inter-core mechanisms require embedded/SoC sources:

1. **Primary:** Upstream Linux kernel
   - `drivers/rpmsg/`
   - `drivers/mailbox/`
   
2. **Secondary:** NXP BSP repos
   - `nxp-auto-linux/ipc-shm`
   - `nxp-auto-linux/ipc-shm-us`
   - NXP i.MX Linux trees
   
3. **Tertiary:** OpenAMP example repos
   - OpenAMP/open-amp
   - OpenAMP/libmetal

### Expected Kernel Locations

```
drivers/rpmsg/
  rpmsg_core.c          — framework (defines APIs, may have internal calls)
  rpmsg_char.c          — char device driver
  virtio_rpmsg_bus.c    — virtio transport
  
drivers/mailbox/
  mailbox.c             — framework
  <vendor>-mailbox.c    — vendor implementations (imx, ti, stm32, etc.)
```

### Smoke Validation Sequence

1. **Fixture validation:** Run tests against mock fixtures

2. **Linux kernel smoke:**
   ```bash
   ./scripts/smoke-rmap.sh bi-em-1 ../linux boundaries list --kind inter_core_channel
   ```

3. **Targeted inspection:**
   - Verify hits in `drivers/rpmsg/`, `drivers/mailbox/`
   - Check vendor-specific drivers (imx, stm32, ti)

4. **NXP BSP smoke (if available):**
   ```bash
   ./scripts/smoke-rmap.sh bi-em-1 ../nxp-linux boundaries list --kind inter_core_channel
   ```

### Acceptance Criteria

- Fixture tests pass
- Linux kernel produces nonzero inter_core_channel hits
- Hits are in expected driver locations
- Properties verified: channel_kind, scope=unknown, direction
- No obvious false positives
- `smoke-runs/` artifacts produced

## Claims

### This slice claims

- This code uses mailbox/RPMsg messaging APIs
- This file is an anchor for inter-core communication
- mbox_send_message / rpmsg_send are provider (outbound) operations

### This slice does NOT claim

- The remote processor is actually running
- Messages are successfully delivered
- Shared memory buffers are correctly configured
- This specific channel connects to a specific remote
- The boundary scope (marked unknown)

## Future Work

- **BI-EM-1B:** Vendor-specific wrapper detection (NXP, TI, ST APIs)
- **BI-EM-1C:** Userspace RPMsg detection (/dev/rpmsg* patterns)
- **BI-EM-1D:** Device tree correlation for channel configuration
- **BI-EM-1E:** Shared memory region correlation (reserved-memory)
- **BI-EM-1F:** Inter-core scope concept (if warranted by usage)
- **BI-EM-1G:** Remoteproc lifecycle detection (separate slice, not messaging)
