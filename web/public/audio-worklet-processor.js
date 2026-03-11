// AudioWorklet processor for Game Boy audio output.
//
// Receives interleaved stereo f32 samples from the main thread via postMessage.
// Buffers them internally and outputs at the device sample rate.

class GameBoyAudioProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    // Internal ring buffer (interleaved L,R)
    this._buffer = new Float32Array(16384);
    this._readPos = 0;
    this._writePos = 0;
    this._count = 0;

    this.port.onmessage = (e) => {
      if (e.data.type === "samples") {
        const samples = e.data.samples;
        const bufLen = this._buffer.length;
        for (let i = 0; i < samples.length; i++) {
          if (this._count < bufLen) {
            this._buffer[this._writePos] = samples[i];
            this._writePos = (this._writePos + 1) % bufLen;
            this._count++;
          }
        }
      }
    };
  }

  process(inputs, outputs, parameters) {
    const output = outputs[0];
    if (!output || output.length < 2) return true;

    const left = output[0];
    const right = output[1];
    const bufLen = this._buffer.length;

    for (let i = 0; i < left.length; i++) {
      if (this._count >= 2) {
        left[i] = this._buffer[this._readPos];
        this._readPos = (this._readPos + 1) % bufLen;
        right[i] = this._buffer[this._readPos];
        this._readPos = (this._readPos + 1) % bufLen;
        this._count -= 2;
      } else {
        left[i] = 0;
        right[i] = 0;
      }
    }

    return true;
  }
}

registerProcessor("gameboy-audio-processor", GameBoyAudioProcessor);
