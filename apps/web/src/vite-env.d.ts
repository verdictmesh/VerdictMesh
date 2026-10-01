/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Where `apps/api` answers, e.g. `https://verdictmesh-api.onrender.com`. */
  readonly VITE_API_URL?: string
  /** The page's own RPC: a key restricted to the site's domain, never the service's. */
  readonly VITE_SOLANA_RPC_URL?: string
  readonly VITE_VERDICT_MESH_PROGRAM_ID?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
