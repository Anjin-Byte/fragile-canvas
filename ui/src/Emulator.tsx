import { useState, useRef, useCallback, useEffect } from "react";
import type { CpuState, EmulatorBackend } from "./types";
import "./global.css";
import "./Emulator.css";

function hex16(n: number) {
  return n.toString(16).toUpperCase().padStart(4, "0");
}

function hex8(n: number) {
  return n.toString(16).toUpperCase().padStart(2, "0");
}

function hexByte(b: number) {
  return b.toString(16).toUpperCase().padStart(2, "0");
}

const FLAG_NAMES = ["Z", "N", "H", "C"] as const;

function Flags({ af }: { af: number }) {
  const f = af & 0xff;
  return (
    <div className="flags">
      {FLAG_NAMES.map((name, i) => {
        const set = (f & (1 << (7 - i))) !== 0;
        return (
          <span key={name} className={`flag ${set ? "flag-set" : "flag-clear"}`}>
            {name}
          </span>
        );
      })}
    </div>
  );
}

function MemoryViewer({
  backend,
  loaded,
}: {
  backend: EmulatorBackend;
  loaded: boolean;
}) {
  const [addr, setAddr] = useState("0000");
  const [data, setData] = useState<number[] | null>(null);

  const fetch = useCallback(async () => {
    if (!loaded) return;
    const base = parseInt(addr, 16);
    if (isNaN(base)) return;
    try {
      const mem = await backend.readMemory(base & 0xffff, 256);
      setData(mem);
    } catch {
      setData(null);
    }
  }, [backend, loaded, addr]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  const baseAddr = parseInt(addr, 16) || 0;

  return (
    <div className="memory-viewer">
      <div className="memory-header">
        <label>
          Addr
          <input
            className="addr-input"
            type="text"
            maxLength={4}
            value={addr}
            onChange={(e) => setAddr(e.target.value.replace(/[^0-9a-fA-F]/g, "").slice(0, 4))}
          />
        </label>
        <button onClick={fetch}>Refresh</button>
      </div>
      {data && (
        <pre className="hex-dump">
          {Array.from({ length: Math.ceil(data.length / 16) }, (_, row) => {
            const offset = row * 16;
            const rowAddr = hex16((baseAddr + offset) & 0xffff);
            const bytes = data.slice(offset, offset + 16);
            const hex = bytes.map((b) => hexByte(b)).join(" ");
            const ascii = bytes
              .map((b) => (b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : "."))
              .join("");
            return `${rowAddr}  ${hex.padEnd(47)}  ${ascii}`;
          }).join("\n")}
        </pre>
      )}
    </div>
  );
}

export function Emulator({ backend }: { backend: EmulatorBackend }) {
  const [cpu, setCpu] = useState<CpuState | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  const runningRef = useRef(false);
  const rafRef = useRef<number>(0);
  const backendRef = useRef(backend);
  backendRef.current = backend;

  const startLoop = useCallback(() => {
    if (runningRef.current) return;
    runningRef.current = true;
    setRunning(true);
    backendRef.current.resetGovernor();

    let last = performance.now();
    function frame(now: number) {
      if (!runningRef.current) return;
      const dt = now - last;
      last = now;
      const elapsedNs = BigInt(Math.round(dt * 1_000_000));
      backendRef.current
        .tickFrame(elapsedNs)
        .then((state) => {
          if (!runningRef.current) return;
          setCpu(state);
          rafRef.current = requestAnimationFrame(frame);
        })
        .catch((e) => {
          runningRef.current = false;
          setRunning(false);
          setError(String(e));
        });
    }
    rafRef.current = requestAnimationFrame(frame);
  }, []);

  const stopLoop = useCallback(() => {
    runningRef.current = false;
    setRunning(false);
    cancelAnimationFrame(rafRef.current);
  }, []);

  useEffect(() => {
    return () => {
      runningRef.current = false;
      cancelAnimationFrame(rafRef.current);
    };
  }, []);

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
    stopLoop();
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
            <button
              className={running ? "btn-active" : ""}
              onClick={running ? stopLoop : startLoop}
            >
              {running ? "Pause" : "Run"}
            </button>
            <button onClick={() => stepCpu(1)} disabled={running}>Step</button>
            <button onClick={() => stepCpu(10)} disabled={running}>Step 10</button>
            <button onClick={() => stepCpu(100)} disabled={running}>Step 100</button>
            <button onClick={() => stepCpu(1000)} disabled={running}>Step 1K</button>
            <button onClick={resetCpu}>Reset</button>
          </div>

          {cpu && (
            <div className="state-panels">
              <div className="registers-panel">
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
                <Flags af={cpu.af} />
              </div>

              <MemoryViewer backend={backend} loaded={loaded} />
            </div>
          )}
        </div>
      )}

      {error && <p className="error">{error}</p>}
    </div>
  );
}
