import { Emulator } from "@fragile-canvas/ui";
import { tauriBackend } from "./backend";

function App() {
  return <Emulator backend={tauriBackend} />;
}

export default App;
