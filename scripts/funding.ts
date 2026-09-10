import { PublicKey } from '@solana/web3.js'

/**
 * Роздача стейку присяжним рахується в базових одиницях мінта від початку до
 * кінця. parseFloat тут коштував би грошей: 0.1 + 0.2 у подвійній точності дає
 * 0.30000000000000004, і на великих сумах помилка перестає бути мікроскопічною.
 */
export function toBaseUnits(amount: string, decimals: number): bigint {
  const trimmed = amount.trim()
  if (!/^\d+(\.\d+)?$/.test(trimmed)) {
    if (/^-/.test(trimmed)) throw new Error(`Amount must be positive: ${amount}`)
    throw new Error(`Amount is not a plain decimal number: ${amount}`)
  }

  const [whole = '0', fraction = ''] = trimmed.split('.')
  if (fraction.length > decimals) {
    throw new Error(`Amount has ${fraction.length} decimal places, mint has ${decimals}: ${amount}`)
  }

  return BigInt(whole + fraction.padEnd(decimals, '0'))
}

/**
 * Зворотне до toBaseUnits, і потрібне рівно в одному місці — у повідомленні про
 * нестачу. Людина, яка йде до крану, оперує «200 USDC», а не «200000000».
 */
export function fromBaseUnits(value: bigint, decimals: number): string {
  const divisor = 10n ** BigInt(decimals)
  const whole = value / divisor
  const fraction = (value % divisor).toString().padStart(decimals, '0').replace(/0+$/, '')
  return fraction === '' ? whole.toString() : `${whole}.${fraction}`
}

/**
 * Скільки бракує до цільового балансу. Нуль, якщо вже достатньо — саме це
 * робить повторний запуск скрипта безпечним.
 */
export function shortfall(current: bigint, target: bigint): bigint {
  return current >= target ? 0n : target - current
}

export function totalRequired(shortfalls: readonly bigint[]): bigint {
  return shortfalls.reduce((sum, value) => sum + value, 0n)
}

export function parseJurorList(text: string): PublicKey[] {
  const seen = new Set<string>()
  const jurors: PublicKey[] = []

  text.split(/\r?\n/).forEach((raw, index) => {
    const line = raw.trim()
    if (line === '' || line.startsWith('#')) return

    let key: PublicKey
    try {
      key = new PublicKey(line)
    } catch {
      throw new Error(`Not a valid address on line ${index + 1}: ${line}`)
    }

    // Присяжний мусить підписувати: стейк, відбиток голосу, розкриття. Адреса
    // поза кривою (PDA) приватного ключа не має за побудовою, тож присяжним
    // бути не може взагалі. Без цієї перевірки помилка спливає аж усередині
    // spl-token, стектрейсом про ATA — тобто про наслідок, а не про причину.
    if (!PublicKey.isOnCurve(key.toBuffer())) {
      throw new Error(
        `Address on line ${index + 1} cannot sign, so it cannot be a juror: ${line}`,
      )
    }

    if (seen.has(line)) {
      throw new Error(`Duplicate juror on line ${index + 1}: ${line}`)
    }
    seen.add(line)
    jurors.push(key)
  })

  return jurors
}
