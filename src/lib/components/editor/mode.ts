import { writable } from "svelte/store";

export const editorMode = writable<"rich" | "plain">("rich");
