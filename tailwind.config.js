/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        muster: {
          bg: "var(--muster-bg, #1c2128)",
          float: "var(--muster-bg-float, #24292f)",
          panel: "var(--muster-panel, rgba(255,255,255,.03))",
          sidebar: "var(--muster-sidebar, #24292f)",
          accent: "var(--muster-accent, #58a6ff)",
          divider: "var(--muster-divider, #30363d)",
          fg: "var(--muster-fg, #e6edf3)",
          muted: "var(--muster-muted, #7d8590)",
          hover: "var(--muster-hover, rgba(255,255,255,.05))",
          "hover-btn": "var(--muster-hover-btn, rgba(255,255,255,.08))",
          selected: "var(--muster-selected, rgba(255,255,255,.10))",
          hairline: "var(--muster-hairline, rgba(255,255,255,.08))",
          border: "var(--muster-border, rgba(255,255,255,.06))",
          "border-strong": "var(--muster-border-strong, rgba(255,255,255,.12))",
          card: "var(--muster-card, rgba(255,255,255,.02))",
          "fill-weak": "var(--muster-fill-weak, rgba(255,255,255,.04))",
          fill: "var(--muster-fill, rgba(255,255,255,.06))",
          "fill-strong": "var(--muster-fill-strong, rgba(255,255,255,.12))",
          scrim: "var(--muster-scrim, rgba(0,0,0,.35))",
          "scrim-dim": "var(--muster-scrim-dim, rgba(0,0,0,.15))",
          well: "var(--muster-well, rgba(0,0,0,.2))",
        },
      },
      boxShadow: {
        pop: "var(--muster-shadow-pop, 0 12px 32px rgba(0,0,0,0.5))",
        "pop-sm": "var(--muster-shadow-pop-sm, 0 8px 24px rgba(0,0,0,0.4))",
        "pop-lg": "var(--muster-shadow-pop-lg, 0 18px 48px rgba(0,0,0,0.55))",
      },
      transitionDuration: {
        muster: "140ms",
      },
      transitionTimingFunction: {
        muster: "cubic-bezier(.2,.8,.3,1)",
      },
    },
  },
  plugins: [],
};