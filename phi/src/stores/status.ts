import { writable } from "svelte/store";

export const hint = writable<string>("");

export function setHint(text: string) {
  hint.set(text);
}

export function clearHint() {
  hint.set("");
}
