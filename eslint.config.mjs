// @ts-check
import withNuxt from './.nuxt/eslint.config.mjs'

export default withNuxt(
  {
    ignores: ['src-tauri/target/**']
  }
  // Your custom configs here
)
