// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  modules: ['@nuxt/eslint', '@nuxt/ui'],

  // Enable SSG
  ssr: false,

  devtools: {
    enabled: false
  },

  css: ['~/assets/css/main.css'],

  // Avoids error [unhandledRejection] EMFILE: too many open files, watch
  ignore: ['**/src-tauri/**'],

  routeRules: {
    '/': { prerender: true }
  },
  // Enables the development server to be discoverable by other devices when running on iOS physical devices
  devServer: {
    host: '0.0.0.0'
  },

  compatibilityDate: '2026-06-30',

  vite: {
    // Better support for Tauri CLI output
    clearScreen: false,
    // Enable environment variables
    // Additional environment variables can be found at
    // https://v2.tauri.app/reference/environment-variables/
    envPrefix: ['VITE_', 'TAURI_'],
    server: {
      // Tauri requires a consistent port
      strictPort: true
    }
  },

  eslint: {
    config: {
      stylistic: {
        commaDangle: 'never',
        braceStyle: '1tbs'
      }
    }
  }
})
