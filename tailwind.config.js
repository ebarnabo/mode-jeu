/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: "#0E0F12",
        surface: "#16181D",
        raised: "#1D2027",
        line: "#272B34",
        muted: "#8A8F9A",
        paper: "#F2F3F5",
        brass: "#D6A64A",
        jade: "#5FB98C",
      },
      fontFamily: {
        sans: ["Space Grotesk", "system-ui", "sans-serif"],
        display: ["Space Grotesk", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "monospace"],
      },
      borderRadius: { xl: "16px", "2xl": "24px", "3xl": "32px" },
      keyframes: {
        rise: { from: { opacity: "0", transform: "translateY(8px)" }, to: { opacity: "1", transform: "none" } },
      },
      animation: { rise: "rise .35s cubic-bezier(.22,1,.36,1) both" },
    },
  },
  plugins: [],
};
