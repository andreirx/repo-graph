# D-DFSR-STDIO-DETAIL-1 — the forced-stdio service note states the configured transport, not a successful daemon spawn

Raised: 2026-10-05 by the requirements reviewer of DOCTOR-FALLBACK-STATE-ROOT-1-PREP (cycle 5).

**The problem in plain words.** RG-REQ-011-L12 revision 7 bound the forced-stdio service-note detail `the client runs its own stdio daemon; this service did not serve this run`. The service probe is built BEFORE any connection (`platform/macos.rs doctor_probes`), and `DaemonClient::ensure_connected()` can fail while spawning `rmapd --stdio` (`daemon_client/mod.rs:175-190`). If the spawn fails, the detail would still say the client *runs* its own daemon — a claim the configuration cannot prove. The slice's own criterion forbids unproven claims (RG-REQ-002-L02/L08, D-AGENT-USEFULNESS-FRAME-1).

**Options (reward / risk).**
- A — revise the phrase to the configured fact: `stdio transport configured; this service is not used by a stdio client`. True whether or not the spawn succeeds (a client configured for stdio never uses the launchd socket service). Reward: keeps the early note and its small data flow; no unproven claim. Risk: an L12 wording revision and another bootstrap re-pin.
- B — show the phrase only after a successful stdio exchange. Reward: keeps the stronger sentence when proved. Risk: the connection result must flow into the probe list — a new data flow; the detail is unavailable until after the socket probes, defeating the "first" order.
- C — defer. Risk: the misleading `doctor` stays.

**Resolved: 2026-10-05 by the OPERATOR (in-place manager) as A**, the reviewer's recommendation; the human may override. RG-REQ-011-L12 revision 8 binds the phrase; the neutral detail for a configured non-global root is unchanged.
