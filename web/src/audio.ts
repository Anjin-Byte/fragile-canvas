// Audio manager for the web frontend.
//
// Sets up an AudioWorklet processor and feeds it interleaved stereo f32
// samples via postMessage. No SharedArrayBuffer required.

export class AudioManager {
  private ctx: AudioContext | null = null;
  private workletNode: AudioWorkletNode | null = null;
  private gainNode: GainNode | null = null;
  private started = false;
  // Desired master gain, retained so a value set before the (lazy) init
  // survives and is applied when the graph is built.
  private volume = 1;

  async init(): Promise<void> {
    if (this.ctx) return;

    this.ctx = new AudioContext({ sampleRate: 48000 });

    // Load the worklet processor
    const base = import.meta.env.BASE_URL || "/";
    await this.ctx.audioWorklet.addModule(`${base}audio-worklet-processor.js`);

    this.workletNode = new AudioWorkletNode(this.ctx, "gameboy-audio-processor", {
      outputChannelCount: [2],
    });
    // worklet → gain → destination, so master volume is a single dial.
    this.gainNode = this.ctx.createGain();
    this.gainNode.gain.value = this.volume;
    this.workletNode.connect(this.gainNode);
    this.gainNode.connect(this.ctx.destination);

    this.started = true;
  }

  /** Master output volume, 0–1. Retained across lazy init. */
  setVolume(volume: number): void {
    this.volume = Math.max(0, Math.min(1, volume));
    if (this.gainNode) this.gainNode.gain.value = this.volume;
  }

  // Push interleaved f32 samples (L,R,L,R...) to the worklet.
  pushSamples(samples: Float32Array): void {
    if (!this.workletNode || samples.length === 0) return;
    this.workletNode.port.postMessage(
      { type: "samples", samples },
      { transfer: [samples.buffer] }
    );
  }

  // Resume AudioContext (required after user gesture).
  async resume(): Promise<void> {
    if (this.ctx?.state === "suspended") {
      await this.ctx.resume();
    }
  }

  // Suspend audio output.
  async suspend(): Promise<void> {
    if (this.ctx?.state === "running") {
      await this.ctx.suspend();
    }
  }

  // Clean up resources.
  async close(): Promise<void> {
    this.workletNode?.disconnect();
    this.workletNode = null;
    await this.ctx?.close();
    this.ctx = null;
    this.started = false;
  }

  get isStarted(): boolean {
    return this.started;
  }
}
