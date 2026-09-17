/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/verdict_mesh.json`.
 */
export type VerdictMesh = {
  "address": "8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq",
  "metadata": {
    "name": "verdictMesh",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Protocol-agnostic dispute resolution layer for Solana"
  },
  "docs": [
    "VerdictMesh — протокол-агностичний шар вирішення спорів.",
    "",
    "Програма не тримає коштів інтегратора: ескроу читає акаунт `Dispute` як",
    "звичайний стан і сам розподіляє кошти (див. docs/PLAN.md → «вердикт витягують,",
    "а не проштовхують»). Тому тут немає жодної інструкції, здатної перевести чужі",
    "гроші — FR-014 є властивістю конструкції, а не обіцянкою."
  ],
  "instructions": [
    {
      "name": "commitVote",
      "docs": [
        "Фіксує прихований відбиток голосу присяжного у вікні подання. Самого",
        "голосу в стані немає до розкриття — див. `instructions::commit_vote`."
      ],
      "discriminator": [
        134,
        97,
        90,
        126,
        91,
        66,
        16,
        26
      ],
      "accounts": [
        {
          "name": "juror",
          "docs": [
            "Присяжний платить оренду власного акаунта голосу. Підпис — єдине, що",
            "доводить авторство відбитка: саме гаманець підписанта входить і в адресу",
            "акаунта, і в сам хеш (`crate::vote`)."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "dispute",
          "docs": [
            "Читається, не змінюється. Лічильника поданих відбитків у `Dispute` немає",
            "навмисно: кворум рахується за **розкритими** голосами (`FR-010`), а хто",
            "подав і змовчав — видно з акаунтів голосів, які для слешингу однаково",
            "доведеться перебрати."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        },
        {
          "name": "vote",
          "docs": [
            "`init`, а не `init_if_needed`: відбиток подається один раз. Друге",
            "подання — це не «змінив рішення», а можливість дочекатися чужого",
            "розкриття і переписати свій голос під нього.",
            "",
            "Адреса виводиться з пари «спір + присяжний», тож записати відбиток у",
            "чужий акаунт не можна, а свій — рівно один на спір."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  111,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute"
              },
              {
                "kind": "account",
                "path": "juror"
              }
            ]
          }
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "commitment",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        }
      ]
    },
    {
      "name": "initialize",
      "docs": [
        "Разова ініціалізація протоколу: розрахунковий актив, ключ ролі reporter",
        "і адреса скарбниці. Інструкції, що змінює записане, у програмі немає —",
        "див. `instructions::initialize`."
      ],
      "discriminator": [
        175,
        175,
        109,
        31,
        13,
        152,
        155,
        237
      ],
      "accounts": [
        {
          "name": "payer",
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "docs": [
            "`init` тут і є захистом від повторної ініціалізації: акаунт, що вже має",
            "власника, до створення не допускається."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint",
          "docs": [
            "Спільний розрахунковий актив протоколу — `FR-011a`. Тип перевіряється",
            "зараз, а не на першому переказі стейку: `Config`, що вказує на не-мінт,",
            "зламав би кожну наступну інструкцію з коштами, і слід вів би сюди.",
            "",
            "Підпису не вимагаємо: емітент розрахункового активу до протоколу",
            "стосунку не має."
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "reporter",
          "type": "pubkey"
        },
        {
          "name": "treasury",
          "type": "pubkey"
        }
      ]
    },
    {
      "name": "openDispute",
      "docs": [
        "Відкриває спір над замкненим залишком. Викликає її програма ескроу",
        "власним підписом — див. `instructions::open_dispute`."
      ],
      "discriminator": [
        137,
        25,
        99,
        119,
        23,
        223,
        161,
        42
      ],
      "accounts": [
        {
          "name": "payer",
          "docs": [
            "Оренду акаунтів спору платить той, хто ініціює транзакцію, а не PDA",
            "ескроу: у PDA може не бути лампортів, і вимагати їх від нього означало б",
            "вимагати від інтегратора тримати баланс у чужій програмі."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint",
          "docs": [
            "Депозит іде в розрахунковому активі протоколу, а не в активі спору:",
            "`FR-011a` тримає економіку присяжних незалежною від того, над чим саме",
            "сперечаються сторони."
          ]
        },
        {
          "name": "depositor",
          "docs": [
            "Депозит вносить **сторона, яка відкриває спір** — `FR-026`, і це саме",
            "той, кого спір записує як `claimant`. Рівність тут не формальність: без",
            "неї ескроу міг би відкрити спір «від імені» позивача, а заплатити з",
            "чужого гаманця, і `FR-026a` не мав би на чому триматись — вартість",
            "розгляду несла б людина, яка про спір не знала.",
            "",
            "Підпис проходить крізь CPI: ескроу підписує спір своїм PDA, позивач —",
            "зовнішню транзакцію."
          ],
          "signer": true
        },
        {
          "name": "integrator",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  105,
                  110,
                  116,
                  101,
                  103,
                  114,
                  97,
                  116,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "integrator.authority",
                "account": "integrator"
              }
            ]
          }
        },
        {
          "name": "escrow",
          "docs": [
            "Акаунт ескроу, над коштами якого йде спір. Він і стає `escrow_ref` —",
            "єдиним, за чим ескроу згодом упізнає «свій» спір, перш ніж розподілити",
            "кошти (`FR-012`).",
            "",
            "Тому вимог дві, і жодна не зайва. **Власник** — програма, яку інтегратор",
            "вказав при реєстрації: інакше `escrow_ref` вказував би на що завгодно.",
            "**Підпис** — бо власності замало: акаунт чужої програми може прочитати",
            "будь-хто, і без підпису вистачило б підставити чужий ескроу, щоб",
            "змусити його виконати вигаданий вердикт.",
            "",
            "власник і підпис."
          ],
          "signer": true
        },
        {
          "name": "dispute",
          "docs": [
            "Номер спору бере лічильник інтегратора, а не клієнт: інакше нумерація",
            "перестала б бути щільною, і `dispute_count` не був би правдою про те,",
            "скільки спорів існує.",
            "",
            "Місце виділяється під **розширену** панель — див. `Dispute::space`."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "integrator"
              },
              {
                "kind": "account",
                "path": "integrator.dispute_count",
                "account": "integrator"
              }
            ]
          }
        },
        {
          "name": "depositorTokens",
          "writable": true
        },
        {
          "name": "disputeVault",
          "docs": [
            "Сховище цього спору — і тільки цього. Окреме на спір, а не спільне:",
            "сюди ж ляже апеляційна застава (`FR-021`), яку доведеться повертати",
            "поіменно, а зі спільного сховища «чия саме це сума» не читається.",
            "",
            "Авторитет — `Config`, той самий PDA, що й у сховища стейків: приватного",
            "ключа до нього не існує, тож депозит виходить лише тим шляхом, який",
            "програма підпише сама (`FR-014`)."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "dispute"
              }
            ]
          }
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "claimant",
          "type": "pubkey"
        },
        {
          "name": "respondent",
          "type": "pubkey"
        },
        {
          "name": "amount",
          "type": "u64"
        },
        {
          "name": "claimantClaimHash",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        },
        {
          "name": "respondentClaimHash",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        }
      ]
    },
    {
      "name": "registerIntegrator",
      "docs": [
        "Закріплює за інтегратором політику арбітражу. Політика перевіряється",
        "один раз, тут, і потрапляє в кожен спір знімком — див.",
        "`instructions::register_integrator`."
      ],
      "discriminator": [
        105,
        254,
        83,
        40,
        118,
        70,
        154,
        105
      ],
      "accounts": [
        {
          "name": "authority",
          "writable": true,
          "signer": true
        },
        {
          "name": "integrator",
          "docs": [
            "PDA за ключем власника: закріпити протокол за собою можна лише власним",
            "підписом, і чужий ключ не має куди записати свою політику."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  105,
                  110,
                  116,
                  101,
                  103,
                  114,
                  97,
                  116,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "authority"
              }
            ]
          }
        },
        {
          "name": "config",
          "docs": [
            "Політики без розрахункового активу не буває: `juror_stake` і `deposit`",
            "виражені саме в ньому (`FR-011a`). Присутність `Config` робить порядок",
            "`initialize` → `register_integrator` перевіркою, а не домовленістю."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "escrowProgram",
          "docs": [
            "Програма ескроу інтегратора: за нею `open_dispute` згодом упізнаватиме,",
            "що спір відкриває саме її акаунт. Перевіряємо, що це справді програма —",
            "оновити поле нічим, тож помилка в ньому була б назавжди.",
            "",
            "лише ознака executable."
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "policy",
          "type": {
            "defined": {
              "name": "policy"
            }
          }
        }
      ]
    },
    {
      "name": "revealVote",
      "docs": [
        "Розкриває голос і звіряє його з поданим відбитком. Розбіжність",
        "відхиляється — див. `instructions::reveal_vote`."
      ],
      "discriminator": [
        100,
        157,
        139,
        17,
        186,
        75,
        185,
        149
      ],
      "accounts": [
        {
          "name": "juror",
          "docs": [
            "Підпис присяжного — те, чим акаунт голосу пов'язується з тим, хто його",
            "подавав: гаманець підписанта входить і в адресу акаунта, і в сам хеш.",
            "Тому розкрити чужий голос своїм підписом не можна — ні за присяжного,",
            "який вирішив змовчати, ні проти нього.",
            "",
            "`mut` не потрібне: розкриття не створює акаунтів і не повертає оренди."
          ],
          "signer": true
        },
        {
          "name": "dispute",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        },
        {
          "name": "vote",
          "docs": [
            "Акаунт не закривається після розкриття: слешинг (`FR-011`, `FR-008b`,",
            "T020) читає саме його, щоб відрізнити правильний голос від програного, а",
            "обидва — від мовчання. Оренда повертається присяжному там."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  111,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute"
              },
              {
                "kind": "account",
                "path": "juror"
              }
            ]
          }
        }
      ],
      "args": [
        {
          "name": "choice",
          "type": {
            "defined": {
              "name": "ballot"
            }
          }
        },
        {
          "name": "salt",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        }
      ]
    },
    {
      "name": "selectPanel",
      "docs": [
        "Відбирає панель присяжних для відкритого спору. Нічия інструкція:",
        "результат детермінований і зафіксований ще при відкритті — див.",
        "`instructions::select_panel`."
      ],
      "discriminator": [
        201,
        213,
        164,
        42,
        76,
        78,
        51,
        113
      ],
      "accounts": [
        {
          "name": "dispute",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        },
        {
          "name": "registry",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  103,
                  105,
                  115,
                  116,
                  114,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "slotHashes",
          "docs": [
            "Джерело ентропії — `FR-006`. Читається сирими байтами: 512 записів",
            "сисвара не десеріалізують цілком (див. `panel::entropy_of`).",
            ""
          ],
          "address": "SysvarS1otHashes111111111111111111111111111"
        }
      ],
      "args": []
    },
    {
      "name": "settleStakes",
      "docs": [
        "Слешить програні голоси й мовчання, ділить зібране між тими, хто був",
        "правий, і випускає панель із реєстру — див. `instructions::settle_stakes`."
      ],
      "discriminator": [
        78,
        170,
        60,
        233,
        48,
        236,
        110,
        215
      ],
      "accounts": [
        {
          "name": "dispute",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        },
        {
          "name": "crank",
          "docs": [
            "Хто завгодно. Підпис потрібен лише тому, що оренду закритого сховища",
            "спору треба комусь віддати, і найчесніший отримувач — той, хто взяв на",
            "себе виклик кранка."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint"
        },
        {
          "name": "disputeVault",
          "docs": [
            "Сховище цього спору. Закривається тут — тому й `mut`."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "dispute"
              }
            ]
          }
        },
        {
          "name": "stakeVault",
          "docs": [
            "Сюди переїжджає частка присяжних: `Juror.stake` — запис проти цього",
            "сховища, і зростати він може лише разом із ним."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  116,
                  97,
                  107,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              }
            ]
          }
        },
        {
          "name": "treasuryTokens",
          "docs": [
            "Токен-акаунт скарбниці протоколу — `FR-026b`. Перевіряється за власником",
            "із `Config`, а не за адресою самого акаунта: адреса ATA виводиться з",
            "власника й мінта, обидва вже прив'язані, а зберігати її окремо означало б",
            "друге джерело того самого факту."
          ],
          "writable": true
        },
        {
          "name": "tokenProgram"
        }
      ],
      "args": []
    },
    {
      "name": "stake",
      "docs": [
        "Вносить стейк і додає присяжного до реєстру. Порогу вступу немає:",
        "достатність стейку визначає політика того спору, у панель якого",
        "присяжний потрапляє — див. `instructions::stake`."
      ],
      "discriminator": [
        206,
        176,
        202,
        18,
        200,
        209,
        179,
        108
      ],
      "accounts": [
        {
          "name": "juror",
          "docs": [
            "Присяжний платить оренду власних акаунтів і сам є авторитетом переказу:",
            "стейк іде зі свого гаманця, а не з чужого за дорученням."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint",
          "docs": [
            "Мінт передається акаунтом, бо `transfer_checked` звіряє за ним знаки.",
            "Прив'язка до `Config` робить підміну неможливою: інакше мінт із іншими",
            "знаками перетворив би сто одиниць на одну соту, і програма б цього не",
            "побачила."
          ]
        },
        {
          "name": "registry",
          "docs": [
            "`init_if_needed` тут безпечний і потрібен: адреса реєстру фіксована,",
            "поля перевіряються констрейнтами, а хендлер нічого не скидає — він лише",
            "збільшує лічильник, тому шлях «створили» і шлях «уже було» дають той",
            "самий результат. Створювати реєстр в `initialize` означало б, що ключ,",
            "який ініціалізує протокол, обирає й мить появи реєстру."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  103,
                  105,
                  115,
                  116,
                  114,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "jurorAccount",
          "docs": [
            "`init`, а не `init_if_needed`: один гаманець — один запис у реєстрі.",
            "Другий стейк тим самим ключем має впасти тут, а не переписати індекс."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "juror"
              }
            ]
          }
        },
        {
          "name": "jurorIndex",
          "docs": [
            "Слот у реєстрі виводиться з лічильника, а не з аргументу: для номера",
            "попереду лічильника просто немає адреси, за якою його створити, тож",
            "нумерація лишається щільною. Відбір панелі (`FR-006`) на це й спирається."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114,
                  95,
                  105,
                  100,
                  120
                ]
              },
              {
                "kind": "account",
                "path": "registry.juror_count",
                "account": "jurorRegistry"
              }
            ]
          }
        },
        {
          "name": "jurorTokens",
          "writable": true
        },
        {
          "name": "stakeVault",
          "docs": [
            "Спільне сховище стейків. Авторитет — `Config`, тобто PDA програми:",
            "приватного ключа до нього не існує за побудовою, і вивести кошти можна",
            "лише тим, що програма підпише сама (`FR-014`). `Config` обрано, бо його",
            "канонічний bump уже лежить у стані — кожен майбутній переказ зі сховища",
            "підписується числом, прочитаним з акаунта, а не виведеним заново."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  116,
                  97,
                  107,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              }
            ]
          }
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    },
    {
      "name": "tally",
      "docs": [
        "Підбиває підсумок голосування: вердикт, одноразова ескалація або",
        "статус-кво. Нічия інструкція — див. `instructions::tally`."
      ],
      "discriminator": [
        152,
        106,
        131,
        171,
        155,
        62,
        41,
        7
      ],
      "accounts": [
        {
          "name": "dispute",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        }
      ],
      "args": []
    },
    {
      "name": "unstake",
      "docs": [
        "Повертає стейк і виводить присяжного з реєстру swap-remove'ом. Поки",
        "присяжний тримає нефіналізований спір, виходу немає — див.",
        "`instructions::unstake`."
      ],
      "discriminator": [
        90,
        95,
        107,
        42,
        205,
        124,
        50,
        225
      ],
      "accounts": [
        {
          "name": "juror",
          "docs": [
            "Виходить лише той, хто підписав: запис присяжного виводиться з цього",
            "ключа, тож за чужий вийти нічим."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint"
        },
        {
          "name": "registry",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  103,
                  105,
                  115,
                  116,
                  114,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "jurorAccount",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "juror"
              }
            ]
          }
        },
        {
          "name": "tailIndex",
          "docs": [
            "Останній слот реєстру. Закривається завжди — реєстр щоразу коротшає",
            "рівно на хвіст, і саме тому в ньому не може лишитись слота поза межею",
            "лічильника, який усе ще посилається на присяжного.",
            "",
            "Оренда повертається тому, хто виходить, хоча платив за цей слот хтось",
            "інший. Слоти однакового розміру, тож кожен присяжний вносить оренду",
            "одного `JurorIndex` і одного ж забирає — різниці немає."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114,
                  95,
                  105,
                  100,
                  120
                ]
              },
              {
                "kind": "account",
                "path": "registry.juror_count.saturating_sub(1)",
                "account": "jurorRegistry"
              }
            ]
          }
        },
        {
          "name": "vacatedIndex",
          "docs": [
            "Слот, що звільняється. Відсутній, коли виходить останній — тоді це той",
            "самий акаунт, що `tail_index`, і другим входом його передавати не можна."
          ],
          "writable": true,
          "optional": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114,
                  95,
                  105,
                  100,
                  120
                ]
              },
              {
                "kind": "account",
                "path": "juror_account.index",
                "account": "juror"
              }
            ]
          }
        },
        {
          "name": "mover",
          "docs": [
            "Присяжний із хвоста — той, хто переїжджає у звільнений слот. Адреса",
            "виводиться з гаманця, записаного в самому хвості, тож підставити сюди",
            "чужий запис нічим."
          ],
          "writable": true,
          "optional": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "tail_index.wallet",
                "account": "jurorIndex"
              }
            ]
          }
        },
        {
          "name": "jurorTokens",
          "writable": true
        },
        {
          "name": "stakeVault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  116,
                  97,
                  107,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              }
            ]
          }
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": []
    }
  ],
  "accounts": [
    {
      "name": "config",
      "discriminator": [
        155,
        12,
        170,
        224,
        30,
        250,
        204,
        130
      ]
    },
    {
      "name": "dispute",
      "discriminator": [
        36,
        49,
        241,
        67,
        40,
        36,
        241,
        74
      ]
    },
    {
      "name": "integrator",
      "discriminator": [
        188,
        141,
        128,
        161,
        231,
        248,
        0,
        128
      ]
    },
    {
      "name": "juror",
      "discriminator": [
        209,
        201,
        239,
        217,
        237,
        84,
        189,
        152
      ]
    },
    {
      "name": "jurorIndex",
      "discriminator": [
        219,
        120,
        204,
        197,
        63,
        141,
        239,
        3
      ]
    },
    {
      "name": "jurorRegistry",
      "discriminator": [
        155,
        172,
        97,
        99,
        65,
        160,
        139,
        28
      ]
    },
    {
      "name": "voteCommit",
      "discriminator": [
        125,
        216,
        109,
        1,
        40,
        87,
        250,
        47
      ]
    }
  ],
  "events": [
    {
      "name": "depositCollected",
      "discriminator": [
        102,
        8,
        94,
        59,
        116,
        130,
        69,
        8
      ]
    },
    {
      "name": "disputeEscalated",
      "discriminator": [
        118,
        176,
        240,
        103,
        220,
        243,
        199,
        13
      ]
    },
    {
      "name": "disputeFeeSettled",
      "discriminator": [
        217,
        15,
        74,
        25,
        117,
        245,
        126,
        57
      ]
    },
    {
      "name": "disputeFinalized",
      "discriminator": [
        171,
        117,
        216,
        12,
        227,
        254,
        59,
        184
      ]
    },
    {
      "name": "disputeOpened",
      "discriminator": [
        239,
        222,
        102,
        235,
        193,
        85,
        1,
        214
      ]
    },
    {
      "name": "disputeTallied",
      "discriminator": [
        18,
        5,
        81,
        193,
        105,
        254,
        177,
        123
      ]
    },
    {
      "name": "jurorRewarded",
      "discriminator": [
        143,
        67,
        217,
        153,
        227,
        207,
        27,
        159
      ]
    },
    {
      "name": "jurorSlashed",
      "discriminator": [
        195,
        75,
        4,
        101,
        187,
        246,
        186,
        211
      ]
    },
    {
      "name": "jurorStaked",
      "discriminator": [
        208,
        14,
        146,
        223,
        12,
        40,
        63,
        222
      ]
    },
    {
      "name": "jurorUnstaked",
      "discriminator": [
        75,
        4,
        227,
        180,
        94,
        213,
        54,
        23
      ]
    },
    {
      "name": "panelSelected",
      "discriminator": [
        179,
        183,
        76,
        53,
        253,
        179,
        22,
        208
      ]
    },
    {
      "name": "reportAttested",
      "discriminator": [
        211,
        25,
        116,
        220,
        193,
        39,
        39,
        155
      ]
    },
    {
      "name": "voteCommitted",
      "discriminator": [
        74,
        67,
        158,
        48,
        168,
        230,
        217,
        77
      ]
    },
    {
      "name": "voteRevealed",
      "discriminator": [
        104,
        162,
        140,
        194,
        213,
        217,
        117,
        179
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "invalidPolicy",
      "msg": "Policy parameters are inconsistent"
    },
    {
      "code": 6001,
      "name": "registryTooSmall",
      "msg": "Juror registry has fewer jurors than the panel requires"
    },
    {
      "code": 6002,
      "name": "insufficientStake",
      "msg": "Juror stake is below the amount required by the policy"
    },
    {
      "code": 6003,
      "name": "jurorLocked",
      "msg": "Juror still participates in an unfinalized dispute"
    },
    {
      "code": 6004,
      "name": "notOnPanel",
      "msg": "Signer is not on the panel for this dispute"
    },
    {
      "code": 6005,
      "name": "wrongState",
      "msg": "Action is not allowed in the current dispute state"
    },
    {
      "code": 6006,
      "name": "windowClosed",
      "msg": "The window for this action has closed"
    },
    {
      "code": 6007,
      "name": "windowOpen",
      "msg": "The window for this action has not opened yet"
    },
    {
      "code": 6008,
      "name": "commitmentMismatch",
      "msg": "Revealed vote does not match the submitted commitment"
    },
    {
      "code": 6009,
      "name": "notReporter",
      "msg": "Report fingerprint may only be written by the reporter role"
    },
    {
      "code": 6010,
      "name": "reportAlreadyAttested",
      "msg": "Report fingerprint is already set and cannot be changed"
    },
    {
      "code": 6011,
      "name": "alreadyEscalated",
      "msg": "Dispute has already been escalated once"
    },
    {
      "code": 6012,
      "name": "aboveOptimisticThreshold",
      "msg": "Dispute amount is above the optimistic threshold"
    },
    {
      "code": 6013,
      "name": "overflow",
      "msg": "Arithmetic overflow"
    },
    {
      "code": 6014,
      "name": "invalidReporter",
      "msg": "Reporter role cannot be the default key"
    },
    {
      "code": 6015,
      "name": "invalidParties",
      "msg": "A dispute needs two different parties"
    },
    {
      "code": 6016,
      "name": "invalidAmount",
      "msg": "A dispute must be opened over a non-zero locked amount"
    },
    {
      "code": 6017,
      "name": "missingClaim",
      "msg": "Both parties must submit a statement fingerprint"
    },
    {
      "code": 6018,
      "name": "invalidRegistryTail",
      "msg": "Registry tail accounts do not match the slot being vacated"
    },
    {
      "code": 6019,
      "name": "entropyUnavailable",
      "msg": "The entropy slot of this dispute is no longer in SlotHashes"
    },
    {
      "code": 6020,
      "name": "invalidPanelAccounts",
      "msg": "The juror accounts do not enumerate the registry"
    },
    {
      "code": 6021,
      "name": "panelAlreadySelected",
      "msg": "The panel for this dispute has already been selected"
    },
    {
      "code": 6022,
      "name": "alreadyRevealed",
      "msg": "This vote has already been revealed"
    },
    {
      "code": 6023,
      "name": "staleCommitment",
      "msg": "A commitment from an earlier round can no longer be revealed"
    },
    {
      "code": 6024,
      "name": "invalidSettlementAccounts",
      "msg": "The juror accounts do not enumerate the panel of this dispute"
    },
    {
      "code": 6025,
      "name": "invalidTreasury",
      "msg": "Protocol treasury cannot be the default key"
    },
    {
      "code": 6026,
      "name": "notTheDepositor",
      "msg": "The arbitration deposit is paid by the party opening the dispute"
    }
  ],
  "types": [
    {
      "name": "ballot",
      "docs": [
        "Бюлетень присяжного — **два варіанти, а не три**.",
        "",
        "`StatusQuo` — це те, чим закінчується невдала ескалація (`FR-027a`), а не",
        "відповідь, яку можна подати. Різниця не формальна: на панелі з трьох при",
        "кворумі два три різні відповіді ніколи не дають більшості, тож третій",
        "варіант у бюлетені був би способом одноосібно відправити будь-який спір на",
        "повторний розгляд — за чужий рахунок, бо ескалацію оплачує протокол",
        "(`FR-027`).",
        "",
        "Окремий тип, а не перевірка в рантаймі, бо перевірити подання неможливо:",
        "`commit_vote` бачить лише хеш. Присяжний, який зафіксував відбиток",
        "`StatusQuo`, дізнався б про заборону аж при розкритті — коли міняти вже",
        "нічого, і мовчання коштує йому більшої частки стейку (`FR-008b`). Типом",
        "такий відбиток просто не виражається, тож пастки не існує."
      ],
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "claimant"
          },
          {
            "name": "respondent"
          }
        ]
      }
    },
    {
      "name": "config",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "settlementMint",
            "type": "pubkey"
          },
          {
            "name": "reporter",
            "docs": [
              "Єдиний привілейований ключ у системі. Може лише записати відбиток звіту —",
              "FR-017a. Інструкцій, що змінюють вердикт чи рухають кошти, для нього немає."
            ],
            "type": "pubkey"
          },
          {
            "name": "treasury",
            "docs": [
              "Власник токен-акаунта, куди йде частка протоколу в оплаті розгляду",
              "(`FR-026b`). Це **адреса призначення, а не роль**: жодна інструкція не",
              "питає в неї підпису й не дає їй нічого, окрім права отримати переказ.",
              "Тому вона й не суперечить `FR-014` — ключ, що не може нічого підписати,",
              "не має влади над вердиктом.",
              "",
              "Записується разом із рештою `Config` і теж не оновлюється: ключ, здатний",
              "переставити отримувача комісії, переставив би його й посеред розгляду."
            ],
            "type": "pubkey"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "depositCollected",
      "docs": [
        "Депозит за розгляд внесено — `FR-026`. Сума в події тому, що вона є",
        "знімком політики на момент відкриття: інтегратор змінить `Policy` завтра, а",
        "сторона має бачити, скільки з неї взяли сьогодні (`FR-026d`)."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "depositor",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "dispute",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "integrator",
            "type": "pubkey"
          },
          {
            "name": "disputeId",
            "type": "u64"
          },
          {
            "name": "policy",
            "docs": [
              "Знімок, не посилання — FR-003."
            ],
            "type": {
              "defined": {
                "name": "policy"
              }
            }
          },
          {
            "name": "escrowRef",
            "docs": [
              "PDA ескроу, який відкрив спір. Ескроу звіряє це поле зі своїм адресом,",
              "перш ніж виконувати вердикт — інакше чужий спір міг би розпорядитись",
              "його коштами."
            ],
            "type": "pubkey"
          },
          {
            "name": "claimant",
            "type": "pubkey"
          },
          {
            "name": "respondent",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "state",
            "type": {
              "defined": {
                "name": "disputeState"
              }
            }
          },
          {
            "name": "panel",
            "docs": [
              "`max_len(0)` — не помилка і не «панель на нуль присяжних». Вектор росте",
              "до `policy.extended_panel_size`, який відомий лише в момент відкриття,",
              "тому `InitSpace` рахує тут саме 4-байтовий префікс довжини, а решту",
              "додає `Dispute::space`. Так фіксовану частину все одно рахує макрос, і",
              "нове поле не може мовчки випасти з розрахунку."
            ],
            "type": {
              "vec": "pubkey"
            }
          },
          {
            "name": "reportHash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "claimantClaimHash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "respondentClaimHash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "openedAt",
            "type": "i64"
          },
          {
            "name": "entropySlot",
            "docs": [
              "Слот, чий хеш дає ентропію для відбору панелі — `FR-006`. Записується",
              "при відкритті і більше не змінюється: якби відбір брав ентропію з",
              "моменту **свого** виконання, його можна було б переграти, повторюючи",
              "спробу зі слота в слот, доки панель не сподобається.",
              "",
              "Це слот **перед** тим, у якому відкрито спір: хеш поточного слота ще не",
              "існує, тож відбір у тій самій транзакції його не знайшов би."
            ],
            "type": "u64"
          },
          {
            "name": "commitDeadline",
            "type": "i64"
          },
          {
            "name": "revealDeadline",
            "type": "i64"
          },
          {
            "name": "appealDeadline",
            "type": "i64"
          },
          {
            "name": "votesClaimant",
            "docs": [
              "Розкриті голоси, по одному лічильнику на варіант бюлетеня. Окремого",
              "`revealed_count` тут немає з тієї ж причини, з якої в `VoteCommit` немає",
              "`revealed`: він дорівнював би сумі цих двох завжди, а два джерела однієї",
              "величини рано чи пізно розходяться. Скільки розкрилось — питають у суми."
            ],
            "type": "u8"
          },
          {
            "name": "votesRespondent",
            "type": "u8"
          },
          {
            "name": "escalated",
            "docs": [
              "Автоескалація застосовується один раз — FR-027a."
            ],
            "type": "bool"
          },
          {
            "name": "verdict",
            "type": {
              "option": {
                "defined": {
                  "name": "verdict"
                }
              }
            }
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "disputeEscalated",
      "docs": [
        "Спір пішов на розширену панель — `FR-027`. Голоси, що спричинили ескалацію,",
        "у самій події: інакше сторона бачить, що розгляд подовжився, і не бачить",
        "чому.",
        "",
        "Списку тих, хто не розкрився, тут немає навмисно, хоча `FR-027b` саме їх і",
        "слешить. Підрахунок їх не знає — він бачить два числа, а не акаунти голосів,",
        "— і тягнути заради події всю панель акаунтами означало б впертись у ліміт",
        "транзакції там, де спостерігач однаково виводить цей список сам: `FR-029`",
        "дає йому `VoteCommitted` без парного `VoteRevealed`."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "votesClaimant",
            "type": "u8"
          },
          {
            "name": "votesRespondent",
            "type": "u8"
          },
          {
            "name": "commitDeadline",
            "type": "i64"
          },
          {
            "name": "revealDeadline",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "disputeFeeSettled",
      "docs": [
        "Куди розійшлась оплата розгляду — `FR-026b`. Двох чисел досить, щоб звести",
        "баланс сховища спору: разом вони дорівнюють тому, що в ньому лежало, а сам",
        "акаунт після цього закривається."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "jurors",
            "type": "u64"
          },
          {
            "name": "protocol",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "disputeFinalized",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "verdict",
            "type": {
              "defined": {
                "name": "verdict"
              }
            }
          },
          {
            "name": "finalizedAt",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "disputeOpened",
      "docs": [
        "FR-029: за подіями зовнішній спостерігач відновлює повну хронологію спору",
        "без доступу до офчейн-сервісу. Watcher у apps/api читає саме їх."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "integrator",
            "type": "pubkey"
          },
          {
            "name": "escrowRef",
            "type": "pubkey"
          },
          {
            "name": "claimant",
            "type": "pubkey"
          },
          {
            "name": "respondent",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "optimistic",
            "type": "bool"
          },
          {
            "name": "openedAt",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "disputeState",
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "optimisticPending"
          },
          {
            "name": "committing"
          },
          {
            "name": "revealing"
          },
          {
            "name": "tallied"
          },
          {
            "name": "appealed"
          },
          {
            "name": "finalized"
          }
        ]
      }
    },
    {
      "name": "disputeTallied",
      "docs": [
        "Вердикт винесено — `FR-010`. Числа поруч із результатом, бо вердикт без",
        "підстави сторона перевірити не може."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "verdict",
            "type": {
              "defined": {
                "name": "verdict"
              }
            }
          },
          {
            "name": "votesClaimant",
            "type": "u8"
          },
          {
            "name": "votesRespondent",
            "type": "u8"
          },
          {
            "name": "appealDeadline",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "integrator",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "authority",
            "type": "pubkey"
          },
          {
            "name": "escrowProgram",
            "type": "pubkey"
          },
          {
            "name": "policy",
            "type": {
              "defined": {
                "name": "policy"
              }
            }
          },
          {
            "name": "disputeCount",
            "type": "u64"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "juror",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "wallet",
            "type": "pubkey"
          },
          {
            "name": "stake",
            "docs": [
              "Внесена сума, а не «достатня»: достатність визначає політика того спору,",
              "у панель якого присяжний потрапляє."
            ],
            "type": "u64"
          },
          {
            "name": "activeDisputes",
            "docs": [
              "Скільки нефіналізованих спорів тримають цього присяжного. Поки не нуль —",
              "вивести стейк не можна (`FR-007`, T015)."
            ],
            "type": "u16"
          },
          {
            "name": "index",
            "docs": [
              "Місце в реєстрі. Разом із `JurorIndex` дає відбору перелічуваність:",
              "`index` веде від присяжного до слота, `JurorIndex` — назад."
            ],
            "type": "u32"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "jurorIndex",
      "docs": [
        "Дає реєстру перелічуваність за індексом — без цього детермінований відбір",
        "(FR-006) не може вибрати N із M, не читаючи весь реєстр офчейн.",
        "Вихід присяжного — swap-remove: останній індекс переїжджає на звільнений."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "wallet",
            "type": "pubkey"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "jurorRegistry",
      "docs": [
        "Реєстр один на протокол, а не на інтегратора: присяжний вносить стейк раз і",
        "потрапляє в панелі всіх інтеграторів. Тому порогу стейку тут немає й бути не",
        "може — `juror_stake` живе в `Policy`, тобто у кожного інтегратора свій.",
        "Придатність присяжного до конкретного спору перевіряє відбір панелі",
        "(`FR-006`, T016), а не вступ до реєстру."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "jurorCount",
            "type": "u32"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "jurorRewarded",
      "docs": [
        "Кому дісталось злетіле зі стейків — `FR-011`. Разом із `JurorSlashed` дає",
        "повний баланс розрахунку: спостерігач бачить, що вийшло і куди пішло, не",
        "читаючи акаунтів."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "jurorSlashed",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "noReveal",
            "type": "bool"
          }
        ]
      }
    },
    {
      "name": "jurorStaked",
      "docs": [
        "Вступ до реєстру присяжних. Watcher (T027) будує з цих подій список",
        "придатних присяжних, не читаючи всі акаунти програми: `getProgramAccounts`",
        "на кожному відборі — це те, чого `JurorIndex` і уникає."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "stake",
            "type": "u64"
          },
          {
            "name": "index",
            "type": "u32"
          },
          {
            "name": "jurorCount",
            "type": "u32"
          }
        ]
      }
    },
    {
      "name": "jurorUnstaked",
      "docs": [
        "Вихід із реєстру разом зі swap-remove. `index` — слот, що звільнився,",
        "`moved` — присяжний, який на нього переїхав із хвоста. Пари подій",
        "`JurorStaked` / `JurorUnstaked` досить, щоб відтворити склад реєстру",
        "цілком — `FR-029`."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "stake",
            "type": "u64"
          },
          {
            "name": "index",
            "type": "u32"
          },
          {
            "name": "moved",
            "type": {
              "option": "pubkey"
            }
          },
          {
            "name": "jurorCount",
            "type": "u32"
          }
        ]
      }
    },
    {
      "name": "panelSelected",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "panel",
            "type": {
              "vec": "pubkey"
            }
          },
          {
            "name": "entropySlot",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "policy",
      "docs": [
        "Копіюється у кожен спір при відкритті. Зміна політики інтегратором не впливає",
        "на вже відкриті спори — FR-003."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "panelSize",
            "type": "u8"
          },
          {
            "name": "extendedPanelSize",
            "type": "u8"
          },
          {
            "name": "quorum",
            "type": "u8"
          },
          {
            "name": "extendedQuorum",
            "type": "u8"
          },
          {
            "name": "jurorStake",
            "type": "u64"
          },
          {
            "name": "slashBpsWrong",
            "type": "u16"
          },
          {
            "name": "slashBpsNoReveal",
            "type": "u16"
          },
          {
            "name": "commitWindow",
            "type": "i64"
          },
          {
            "name": "revealWindow",
            "type": "i64"
          },
          {
            "name": "appealWindow",
            "type": "i64"
          },
          {
            "name": "optimisticWindow",
            "type": "i64"
          },
          {
            "name": "deposit",
            "type": "u64"
          },
          {
            "name": "optimisticThreshold",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "reportAttested",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "reportHash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          }
        ]
      }
    },
    {
      "name": "verdict",
      "docs": [
        "StatusQuo — «як ніби спору не було» (FR-027a). Ескроу зобов'язаний уміти",
        "розподілити кошти за цим результатом, інакше автоескалація нікуди не веде."
      ],
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "claimant"
          },
          {
            "name": "respondent"
          },
          {
            "name": "statusQuo"
          }
        ]
      }
    },
    {
      "name": "voteCommit",
      "docs": [
        "Відбиток голосу присяжного — `FR-008`. Створюється в вікні подання і до",
        "розкриття не містить нічого, з чого можна вивести сам голос: `commitment` —",
        "хеш (`crate::vote`), `choice` — `None`.",
        "",
        "**Прапорця `revealed` тут немає навмисно**, хоча модель даних у `PLAN.md`",
        "його називає. Він завжди дублював би `choice.is_some()`, а два поля, які",
        "зобов'язані збігатись, рано чи пізно розходяться: розрахунок стейків (T020)",
        "відрізняє нерозкритий голос від розкритого і мусить робити це за одним",
        "джерелом. Ним і є `choice`."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "commitment",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "choice",
            "docs": [
              "`None`, доки голос не розкрито (T018). Порожнє значення — це і є",
              "«присяжний подав відбиток, але не розкрився» для `FR-008b`."
            ],
            "type": {
              "option": {
                "defined": {
                  "name": "ballot"
                }
              }
            }
          },
          {
            "name": "round",
            "docs": [
              "Коло розгляду, у якому подано відбиток — `Dispute::round`. Розкрити його",
              "можна лише в тому ж колі: інакше присяжний, що змовчав у першому,",
              "вирішував би в другому, чи оприлюднювати голос, уже знаючи, чим",
              "закінчився перший підрахунок. `FR-027b` слешить його саме за це",
              "мовчання, і пізнє розкриття не має його скасовувати."
            ],
            "type": "u8"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "voteCommitted",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "voteRevealed",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "choice",
            "type": {
              "defined": {
                "name": "ballot"
              }
            }
          }
        ]
      }
    }
  ]
};
