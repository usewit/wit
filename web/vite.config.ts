import { defineConfig } from "vite";

// In dev, the Rust server handles /api: port 7700, or WIT_API for a scratch one.
export default defineConfig({
  server: { host: true, proxy: { "/api": process.env.WIT_API ?? "http://localhost:7700" } },
});
