import { useState } from "react";
import type { CpuState, EmulatorBackend } from "./types";
import "./global.css";
import "./Emulator.css";

function hex16(n: number) {
  return n.toString(16).toUpperCase().padStart(4, "0");
}

function hex8(n: number) {
  return n.toString(16).toUpperCase().padStart(2, "0");
}

export function Emulator({ backend }: { backend: EmulatorBackend }) {
  const [cpu, setCpu] = useState<CpuState | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function loadFiles() {
    const cartInput = document.getElementById("cart-rom") as HTMLInputElement;

    if (!cartInput.files?.[0]) {
      setError("select a ROM file");
      return;
    }

    const cartBuf = await cartInput.files[0].arrayBuffer();

    try {
      const state = await backend.loadRom(new Uint8Array(cartBuf));
      setCpu(state);
      setLoaded(true);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function loadDefault() {
    try {
      const state = await backend.loadDefaultRom();
      setCpu(state);
      setLoaded(true);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function stepCpu(ticks: number) {
    try {
      const state = await backend.step(ticks);
      setCpu(state);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function resetCpu() {
    await backend.reset();
    setCpu(null);
    setLoaded(false);
    setError(null);
  }

  return (
    <div className="container">
      <h1>fragile-canvas</h1>

      {!loaded ? (
        <div className="rom-loader">
          <label>
            Cartridge
            <input id="cart-rom" type="file" />
          </label>
          <button onClick={loadFiles}>Load</button>
          <button onClick={loadDefault}>Load Default</button>
        </div>
      ) : (
        <div className="debugger">
          <div className="controls">
            <button onClick={() => stepCpu(1)}>Step</button>
            <button onClick={() => stepCpu(10)}>Step 10</button>
            <button onClick={() => stepCpu(100)}>Step 100</button>
            <button onClick={() => stepCpu(1000)}>Step 1K</button>
            <button onClick={resetCpu}>Reset</button>
          </div>

          {cpu && (
            <table className="registers">
              <tbody>
                <tr><td>PC</td><td>{hex16(cpu.pc)}</td></tr>
                <tr><td>SP</td><td>{hex16(cpu.sp)}</td></tr>
                <tr><td>AF</td><td>{hex16(cpu.af)}</td></tr>
                <tr><td>BC</td><td>{hex16(cpu.bc)}</td></tr>
                <tr><td>DE</td><td>{hex16(cpu.de)}</td></tr>
                <tr><td>HL</td><td>{hex16(cpu.hl)}</td></tr>
                <tr><td>IR</td><td>{hex8(cpu.ir)}</td></tr>
                <tr><td>IE</td><td>{hex8(cpu.ie)}</td></tr>
                <tr><td>Halted</td><td>{cpu.halted ? "yes" : "no"}</td></tr>
              </tbody>
            </table>
          )}
        </div>
      )}

      {error && <p className="error">{error}</p>}
    </div>
  );
}
