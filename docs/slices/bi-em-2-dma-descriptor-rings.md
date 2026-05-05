# BI-EM-2: DMA and Descriptor Ring Detection

**Status:** WITHDRAWN  
**Slice:** BI-EM-2  
**Family:** ~~Embedded / Inter-Core~~ (reclassified)

## Withdrawal Notice

**Decision:** This slice has been withdrawn from the boundary interaction track.

**Reason:** DMA API usage (dma_alloc_coherent, dma_map_single, dma_sync_*, etc.)
is hardware I/O plumbing, not software-to-software boundary interaction.

Boundary interactions in repo-graph model communication between software entities:
- IPC (pipes, sockets, shared memory, message queues)
- Network protocols (HTTP, gRPC, AMQP)
- Inter-core messaging (RPMsg, mailbox)

DMA is CPU-to-hardware data movement. The "other side" (NIC firmware, GPU, DMA
controller) is not indexable software. A driver mapping a buffer for a NIC does
not create a "boundary" in the repo-graph sense — it is device I/O, not
software-to-software communication.

## Reclassification

This concept is not deleted, but deferred to a future track:

**Future track:** Hardware Resource Hints (not boundary interaction)

If DMA detection proves valuable for agent orientation in driver code, it should
be surfaced as a separate hint family with different semantics:
- Not `channel_kind` / `boundary_scope` / `direction`
- Different vocabulary appropriate for hardware resource management
- Separate from software-to-software boundary model

## Original Problem Statement (Preserved for Reference)

DMA (Direct Memory Access) and descriptor ring patterns are fundamental to
high-performance I/O in embedded systems, network drivers, and storage stacks.
These represent hardware-mediated data movement that bypasses the CPU.

Agents analyzing driver code could benefit from seeing where DMA coordination
exists. However, this is device I/O orientation, not boundary interaction
discovery.

## APIs That Were Considered

For reference, these DMA APIs were originally scoped:

**DMA Memory Allocation:**
- `dma_alloc_coherent`, `dma_free_coherent`, `dma_alloc_attrs`
- `dma_pool_create`, `dma_pool_destroy`, `dma_pool_alloc`, `dma_pool_free`

**Streaming DMA Mapping:**
- `dma_map_single`, `dma_unmap_single`
- `dma_map_page`, `dma_unmap_page`
- `dma_map_sg`, `dma_unmap_sg`

**DMA Sync Operations:**
- `dma_sync_single_for_cpu`, `dma_sync_single_for_device`
- `dma_sync_sg_for_cpu`, `dma_sync_sg_for_device`

**DMA Engine API:**
- `dmaengine_prep_slave_single`, `dmaengine_prep_slave_sg`
- `dmaengine_submit`, `dma_async_issue_pending`

## Future Work

If hardware resource hints become a product direction:

1. Define a separate hint family with appropriate vocabulary
2. Consider what orientation value DMA detection provides
3. Scope narrowly (e.g., descriptor ring completion queues where software
   coordination exists) rather than every dma_* call
4. Keep separate from boundary interaction model

## See Also

- BI-EM-1: Inter-Core Messaging (mailbox/RPMsg) — retained as boundary interaction
- BI-LX-1/2/3: Linux IPC slices — retained as boundary interaction
