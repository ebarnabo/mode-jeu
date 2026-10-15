/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: "#09090B",
        surface: "#121214",
        raised: "#1C1C1F",
        line: "#2A2A2E",
        muted: "#A1A1AA",
        paper: "#F4F4F5",
        brass: "#E2B15A",
        jade: "#5FB98C",
      },
      fontFamily: {
        sans: ["Space Grotesk", "system-ui", "sans-serif"],
        display: ["Space Grotesk", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "monospace"],
      },
      borderRadius: { xl: "16px", "2xl": "24px", "3xl": "32px" },
    },
  },
  plugins: [],
};
