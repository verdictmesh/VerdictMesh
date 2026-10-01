/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Where `apps/api` answers, e.g. `https://verdictmesh-api.onrender.com`. */
  readonly VITE_API_URL?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
