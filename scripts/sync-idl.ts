/**
 * Копіює типи, які згенерував `anchor build`, у дерево скриптів.
 *
 * `target/` не комітиться, а CI типечекає TypeScript без ончейн-збірки — тож
 * тип програми мусить лежати в репо. Копія тільки для типів: сам IDL скрипт
 * читає з `target/idl/` у момент прогону, бо запускатися без свіжої збірки він
 * однаково не може, і розходження між типом і поверхнею програми має бути
 * червоним типечеком, а не сюрпризом на devnet.
 */

import { copyFileSync } from 'node:fs'

for (const program of ['verdict_mesh', 'reference_escrow']) {
  const from = new URL(`../target/types/${program}.ts`, import.meta.url)
  const to = new URL(`./idl/${program}.ts`, import.meta.url)
  copyFileSync(from, to)
  console.log(`synced ${program}`)
}
