import svelte from 'eslint-plugin-svelte';
import tsParser from '@typescript-eslint/parser';
import svelteParser from 'svelte-eslint-parser';

export default [
  ...svelte.configs['flat/recommended'],
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tsParser,
      },
    },
  },
  {
    ignores: ['build/', '.svelte-kit/', 'dist/', 'src-tauri/', 'node_modules/'],
  },
];
