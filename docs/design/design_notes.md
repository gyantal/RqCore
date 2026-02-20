# Design Notes

## Crate hierarcy of the RqCore workspace

The rqcoresrv crate uses broker-common (ib-api), that uses memdb (for MarkValueCache) that uses rqcommon (for base helpers)

|       tools/*          | rqcoresrv       |
| ---------------------- | --------------- |
|                        | broker-common   |
|                        | memdb           |
|        rqcommon        | rqcommon        |

## Speed difference: RAM vs. SSD vs. Internet

A rough statistics about the speed difference of mem-copy of 10MB raw memory. On high-end PC.
Compare the following 3 options:
    From RAM (not in CPU cache)
    From SSD drive
    From Internet (assuming 1Gbit/sec connection)
What is the approximate time in the different cases in microsecs? What are the multipliers between these 3 cases?


| Source                 | Assumed throughput | Approx time (µs)                       |
| ---------------------- | ------------------ | -------------------------------------- |
| RAM → RAM (not cached) | ~60 GB/s memcpy    | \*\*~150 µs\*\*   (0.15ms)             |
| NVMe SSD → RAM         | ~10 GB/s read      | \*\*~1,000 µs\*\* (1ms)                |
| Internet @ 1 Gbit/s    | 1 Gbit/s (ideal)   | \*\*~80,000 µs\*\* (real-world ~100ms) |

Multipliers (approx):
SSD vs. RAM: ~6x slower (1000 µs / 150 µs)
Internet vs. SSD: ~100x slower (100,000 µs / 1000 µs)
Therefore: Internet vs. RAM: ~600x slower (100,000 µs / 150 µs)

The design choice here is that for fast execution, we want everything in RAM (0.15ms). Reading from SSD is 6x slower (1ms) (For simplicity: consider 10x slower). Reading from Internet is 600x slower (100ms).

