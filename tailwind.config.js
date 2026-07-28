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
        },
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