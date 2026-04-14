<script lang="ts">
  import { Emulator } from "@fragile-canvas/ui";
  import { wasmBackend } from "./backend";
</script>

<Emulator backend={wasmBackend} />
