import { mergeConfig, defineConfig } from 'vite';
import viteConfig from '../vite.config';
import { resolve } from "node:path";

export default defineConfig((env) => mergeConfig(
  viteConfig(env),
  defineConfig({
    //tchap config
   build:{
     rollupOptions: {
      input: [
        resolve(__dirname, "src/tchap.css"),
         ]
      }
   }
}
    
  ),
));