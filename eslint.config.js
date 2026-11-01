// ESLint for the browser UI (web/js/app/, native ES modules served as-is).
// Deliberately small and non-stylistic: undefined names, unused bindings,
// writes to imports/constants, and imports that do not resolve. Formatting is
// not linted. Run: npm ci && npx eslint
import globals from 'globals';
import importExports from './eslint-rules/import-exports.js';

export default [
  { ignores: ['target/**', 'vendor/**', 'web/vendor/**', 'node_modules/**'] },
  {
    files: ['web/js/app/**/*.js'],
    languageOptions: {
      ecmaVersion: 2024,
      sourceType: 'module',
      globals: {
        ...globals.browser,
        // Classic scripts loaded before the modules (web/vendor/xterm).
        Terminal: 'readonly',
        FitAddon: 'readonly',
      },
    },
    linterOptions: { reportUnusedDisableDirectives: 'error' },
    plugins: { local: importExports },
    rules: {
      'no-undef': 'error',
      'no-unused-vars': ['error', { args: 'after-used', caughtErrors: 'none', ignoreRestSiblings: true }],
      'no-import-assign': 'error',
      'no-const-assign': 'error',
      'no-func-assign': 'error',
      'no-redeclare': 'error',
      'no-dupe-keys': 'error',
      'no-unreachable': 'error',
      'local/import-exports': 'error',
    },
  },
  {
    files: ['eslint.config.js', 'eslint-rules/**/*.js'],
    languageOptions: { ecmaVersion: 2024, sourceType: 'module', globals: globals.node },
    rules: { 'no-undef': 'error', 'no-unused-vars': 'error' },
  },
];
