import { PublicKey } from '@solana/web3.js'
import { describe, expect, it } from 'vitest'
import { fromBaseUnits, parseJurorList, shortfall, toBaseUnits, totalRequired } from './funding.js'

describe('toBaseUnits', () => {
  it('converts a whole amount', () => {
    expect(toBaseUnits('100', 6)).toBe(100_000_000n)
  })

  it('converts a fractional amount without going through a float', () => {
    expect(toBaseUnits('100.5', 6)).toBe(100_500_000n)
  })

  it('pads a short fraction rather than truncating it', () => {
    expect(toBaseUnits('0.1', 6)).toBe(100_000n)
  })

  /**
   * 0.1 + 0.2 in binary floating point is 0.30000000000000004. A parseFloat
   * implementation returns 300000.00000000006 here and silently rounds; this
   * asserts we never took that path.
   */
  it('is exact where a float would not be', () => {
    expect(toBaseUnits('0.3', 6)).toBe(300_000n)
    expect(toBaseUnits('4503599627370497.000001', 6)).toBe(4_503_599_627_370_497_000_001n)
  })

  it('rejects more decimals than the mint has', () => {
    expect(() => toBaseUnits('1.1234567', 6)).toThrow(/decimal/i)
  })

  it('rejects a negative amount', () => {
    expect(() => toBaseUnits('-1', 6)).toThrow(/positive/i)
  })

  it('rejects text that is not a number', () => {
    expect(() => toBaseUnits('100 USDC', 6)).toThrow(/amount/i)
  })
})

describe('fromBaseUnits', () => {
  it('drops the fraction when there is none', () => {
    expect(fromBaseUnits(200_000_000n, 6)).toBe('200')
  })

  it('keeps a fraction without trailing zeros', () => {
    expect(fromBaseUnits(100_500_000n, 6)).toBe('100.5')
  })

  it('pads a small fraction to the right place', () => {
    expect(fromBaseUnits(1n, 6)).toBe('0.000001')
  })

  it('round-trips through toBaseUnits', () => {
    for (const amount of ['0', '1', '100.5', '0.000001', '4503599627370497.000001']) {
      expect(fromBaseUnits(toBaseUnits(amount, 6), 6)).toBe(amount)
    }
  })
})

describe('shortfall', () => {
  it('asks for the difference when the account is short', () => {
    expect(shortfall(40n, 100n)).toBe(60n)
  })

  it('asks for nothing when the account is already at target', () => {
    expect(shortfall(100n, 100n)).toBe(0n)
  })

  /** Rerunning the script must not top anyone up a second time. */
  it('asks for nothing when the account is above target', () => {
    expect(shortfall(250n, 100n)).toBe(0n)
  })

  it('asks for the full target when the account is empty', () => {
    expect(shortfall(0n, 100n)).toBe(100n)
  })
})

describe('totalRequired', () => {
  it('sums shortfalls', () => {
    expect(totalRequired([10n, 0n, 25n])).toBe(35n)
  })

  it('is zero for an empty list', () => {
    expect(totalRequired([])).toBe(0n)
  })
})

describe('parseJurorList', () => {
  const a = '8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq'
  const b = '4iYF4WRdtuSmjTXH5fSa2ow5WrdeonEoeoY3epypfTHo'

  it('reads one address per line', () => {
    expect(parseJurorList(`${a}\n${b}\n`).map(String)).toEqual([a, b])
  })

  it('ignores blank lines and comments', () => {
    expect(parseJurorList(`# jurors\n\n${a}\n   \n`).map(String)).toEqual([a])
  })

  it('rejects a duplicate rather than funding it twice', () => {
    expect(() => parseJurorList(`${a}\n${a}`)).toThrow(/duplicate/i)
  })

  it('rejects an address that is not valid base58', () => {
    expect(() => parseJurorList('not-an-address')).toThrow(/line 1/)
  })

  it('names the offending line so a long list is fixable', () => {
    expect(() => parseJurorList(`${a}\n${b}\nnope`)).toThrow(/line 3/)
  })

  // Присяжний мусить підписувати — стейк, відбиток голосу, розкриття. Адреса
  // поза кривою підпису не має за побудовою, тож присяжним бути не може взагалі.
  // Це не незручність токен-акаунта, а неправильний запис у списку.
  it('rejects an address that cannot sign, naming the line', () => {
    const pda = PublicKey.findProgramAddressSync([Buffer.from('vote')], new PublicKey(a))[0]
    expect(PublicKey.isOnCurve(pda.toBuffer())).toBe(false)

    expect(() => parseJurorList(`${a}\n${pda.toBase58()}`)).toThrow(/line 2/)
    expect(() => parseJurorList(pda.toBase58())).toThrow(/sign/i)
  })
})
