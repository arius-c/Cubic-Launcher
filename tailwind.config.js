/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // Colors matching code.html exactly
        primary: {
          DEFAULT: "#9333ea",
          foreground: "#ffffff",
        },
        brandPurple: "#9333ea",
        brandPurpleHover: "#a855f7",
        bgDark: "#121212",
        bgPanel: "#1e1e1e",
        bgHover: "#2a2a2a",
        textMain: "#e5e5e5",
        textMuted: "#a3a3a3",
        borderColor: "#333333",
        // Aliases for compatibility
        background: "#121212",
        foreground: "#e5e5e5",
        card: {
          DEFAULT: "#1e1e1e",
          foreground: "#e5e5e5",
        },
        popover: {
          DEFAULT: "#18181b",
          foreground: "#f4f4f5",
        },
        secondary: {
          DEFAULT: "#27272a",
          foreground: "#f4f4f5",
        },
        muted: {
          DEFAULT: "#27272a",
          foreground: "#a1a1aa",
        },
        accent: {
          DEFAULT: "#9333ea",
          foreground: "#ffffff",
        },
        destructive: {
          DEFAULT: "#ef4444",
          foreground: "#ffffff",
        },
        border: "#333333",
        input: "#27272a",
        ring: "#9333ea",
        sidebar: {
          DEFAULT: "#1e1e1e",
          foreground: "#e5e5e5",
          primary: "#9333ea",
          "primary-foreground": "#ffffff",
          accent: "#2a2a2a",
          "accent-foreground": "#e5e5e5",
          border: "#333333",
          ring: "#9333ea",
        },
        success: {
          DEFAULT: "#22c55e",
          foreground: "#ffffff",
        },
        warning: {
          DEFAULT: "#eab308",
          foreground: "#1a1a1a",
        },
      },
      fontFamily: {
        sans: ['publicSans', 'system-ui', 'sans-serif'],
      },
      borderRadius: {
        lg: "0.5rem",
        md: "calc(0.5rem - 2px)",
        sm: "calc(0.5rem - 4px)",
      },
      boxShadow: {
        glow: "0 -4px 20px rgba(0,0,0,0.3)",
      },
    },
  },
  plugins: [],
}
