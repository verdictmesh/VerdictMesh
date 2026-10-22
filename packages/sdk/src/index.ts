/**
 * VerdictMesh for integrators: the program's IDL and types, account addresses,
 * the vote commitment, and a client that registers a protocol and opens
 * disputes from its escrow, and the rule by which the escrow may execute a
 * verdict (`verdictStatus`, `VerdictMeshClient.readVerdict`).
 *
 * `@verdictmesh/sdk/pda` and `@verdictmesh/sdk/vote` are the same modules
 * without Anchor, for a page that only needs an address or a commitment.
 */

export {
  type DisputeAccounts,
  type Policy,
  type PreparedDispute,
  VerdictMeshClient,
} from './client.js'
export type { VerdictMesh } from './idl/verdict_mesh.js'
export { verdictMeshIdl } from './idl/verdict-mesh.js'
export * from './pda.js'
export * from './verdict.js'
export * from './vote.js'
