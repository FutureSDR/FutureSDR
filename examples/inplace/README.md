In-Place vs. Out-of-Place Processing Performance Example (iplace)
========================

## Introduction

This example explores the performance trade-offs between different memory management strategies in FutureSDR. It compares how data is moved through the flowgraph using three distinct approaches: In-place, Hybrid, and Out-of-place.

## How it works:
1. Out-of-place: Each block reads from an input buffer and writes results to a newly allocated output buffer.
2. In-place: Blocks modify data directly within the same memory buffer. This is achieved through a feedback loop: Once the sink is done, it hands the available buffers back to the source.
3. Hybrid: Utilizes FutureSDR's internal `circuit` buffers. It attempts to combine the ease of use of standard blocks with the speed of virtual memory tricks to minimize the overhead of buffer switching.

## How to run:
* Go to the example directory and run it:

```sh
cargo run --release
  ```

* Once you run the command, the terminal will display the time elapsed for each method to complete the operation.
* Generally, out-of-place is the slowest while the hybrid method is the most efficient. However, if the data vector is relatively short, in-place might actually take longer than the out-of-place method due to management overhead. To see the expected result, try increasing the number of elements in the vector within the source code and running the test again.


