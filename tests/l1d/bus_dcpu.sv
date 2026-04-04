// CVA6-compatible CPU-to-dcache bus.
// Directions declared from the CPU initiator perspective.
// When the cache uses 'port cpu: target BusDcpu', all in/out flip.
// Request channel (CPU → cache)
// Response channel (cache → CPU)
// resp_ready prevents silent response drop when CPU is stalled.
