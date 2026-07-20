<script lang="ts">
  /**
   * Footer — a static, low-profile attribution sliver. It carries no dynamic
   * state: emulation status (run-state + fps) lives in the toolbar, and the
   * debugger details it used to show (PC / LY / frame) are available in the
   * CPU, PPU and I/O panels.
   */
</script>

<footer class="footer">
  <span class="footer-text">
    Okra · Small · Green 
  </span>
</footer>

<style>
  .footer {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 20px;
    background: var(--surface-0);
    flex-shrink: 0;
    user-select: none;
  }

  .footer-text {
    font-family: var(--font-sans);
    font-size: 10px;
    letter-spacing: 0.03em;
    color: var(--text-faint);
    opacity: 0.75;
  }
</style>
