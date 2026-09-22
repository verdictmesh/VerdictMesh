import { defineConfig } from 'drizzle-kit'

/**
 * `generate` мережі не потребує — знімок збирається зі схеми, тож гейт ганяє
 * його без бази. `DATABASE_URL` потрібен лише для `migrate`, і його підставляє
 * деплой. Ходити ним треба через **session pooler**, а не через транзакційний
 * 6543: DDL у транзакційному режимі розкладається на чужі з'єднання.
 */
export default defineConfig({
  dialect: 'postgresql',
  schema: './src/schema.ts',
  out: './migrations',
  dbCredentials: { url: process.env.DATABASE_URL ?? 'postgres://localhost:5432/verdictmesh' },
})
