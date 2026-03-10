import { Emulator } from "@fragile-canvas/ui";
import { wasmBackend } from "./backend";

function App() {
  return <Emulator backend={wasmBackend} />;
}

export default App;
