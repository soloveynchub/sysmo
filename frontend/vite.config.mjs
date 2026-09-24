import {defineConfig} from 'vite';
import {fileURLToPath} from 'node:url';

export default defineConfig({build:{rollupOptions:{input:{
  main:fileURLToPath(new URL('./index.html',import.meta.url)),
  preview:fileURLToPath(new URL('./mock.html',import.meta.url))
}}}});
