// Audio manager for the desktop frontend.
//
// Sets up an AudioWorklet processor and feeds it interleaved stereo f32
// samples via postMessage. No SharedArrayBuffer required.

export class AudioManager {
  private ctx: AudioContext | null = null;
  private workletNode: AudioWorkletNode | null = null;
  private started = false;

  async init(): Promise<void> {
    if (this.ctx) return;

    this.ctx = new AudioContext({ sampleRate: 48000 });

    const base = import.meta.env.BASE_URL || "/";
    await this.ctx.audioWorklet.addModule(`${base}audio-worklet-processor.js`);

    this.workletNode = new AudioWorkletNode(this.ctx, "gameboy-audio-processor", {
      outputChannelCount: [2],
    });
    this.workletNode.connect(this.ctx.destination);

    this.started = true;
  }

  pushSamples(samples: Float32Array): void {
    if (!this.workletNode || samples.length === 0) return;
    this.workletNode.port.postMessage(
      { type: "samples", samples },
      { transfer: [samples.buffer] }
    );
  }

  async resume(): Promise<void> {
    if (this.ctx?.state === "suspended") {
      await this.ctx.resume();
    }
  }

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
