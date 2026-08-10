/** @type {import('prettier').Config} */
const config = {
  plugins: ['prettier-plugin-svelte'],
  singleQuote: true,
  trailingComma: 'none',
  printWidth: 100,
  overrides: [{ files: '*.svelte', options: { parser: 'svelte' } }]
};

export default config;