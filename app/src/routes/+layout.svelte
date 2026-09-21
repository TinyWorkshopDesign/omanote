<script lang="ts">
  import "../app.css";
  import { dev } from "$app/environment";
  import { invoke } from "@tauri-apps/api/core";

  let { children } = $props();

  // Development only: forward webview errors to the `tauri dev` log.
  if (dev && typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    const log = (msg: string) => void invoke("debug_log", { msg }).catch(() => {});
    (window as unknown as { __omanoteLog: typeof log }).__omanoteLog = log;
    window.addEventListener("error", (e) => log(`error: ${e.message} @ ${e.filename}:${e.lineno}`));
    window.addEventListener("unhandledrejection", (e) => log(`rejection: ${String(e.reason)}`));
  }
</script>

{@render children()}
