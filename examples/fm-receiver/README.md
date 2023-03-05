# fm-receiver
A simple FM receiver that you can tune to nearby radio stations

## How it works:
When you run the example, it will build a flowgraph consisting of the following blocks:
* SeifySource: Gets data from your SDR
* Demodulator: Demodulates the FM signal
* AudioSink: Plays the demodulated signal on your device

After giving it some time to start up the SDR, it enters a loop where you will
be periodically asked to enter a new frequency that the SDR will be tuned to.
**Watch out** though: Some frequencies (very high or very low) might be unsupported
by your SDR and may cause a crash.

## How to run:
- plug in your SDR
- go to the example folder and run it
  ```bash
  $ cd FutureSDR/examples/fm-receiver

  $ argo run
  ```
  
- enter a frequency in MHz in the prompt, e.g. `89.1` (and press Enter) to tune the radio to 89.1 MHz. This prompt for a new frequency runs in a loop, so can change the frequency over and over again. 
