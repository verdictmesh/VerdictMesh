/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/reference_escrow.json`.
 */
export type ReferenceEscrow = {
  "address": "4iYF4WRdtuSmjTXH5fSa2ow5WrdeonEoeoY3epypfTHo",
  "metadata": {
    "name": "referenceEscrow",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Milestone escrow that resolves disputes through VerdictMesh"
  },
  "docs": [
    "Milestone-ескроу, який купує арбітраж у VerdictMesh замість того, щоб писати",
    "власний. Це і демо-інтеграція для US1/US2/US4/US5, і зразок, за яким міряється",
    "SC-004.",
    "",
    "Виконання вердикту працює за pull-моделлю: `settle` читає акаунт `Dispute`,",
    "перевіряє власника, стан і те, що спір належить саме цьому ескроу, після чого",
    "розподіляє кошти сам. VerdictMesh при цьому не має жодного повноваження над",
    "цим ескроу."
  ],
  "instructions": [
    {
      "name": "createEscrow",
      "docs": [
        "Замикає всю суму угоди й розписує її по віхах, а разом з нею — заставу за",
        "розгляд з обох сторін (`FR-026e`). Підписують обидві сторони: разом з",
        "угодою вони приймають політику розгляду. Див. `instructions::escrow`."
      ],
      "discriminator": [
        253,
        215,
        165,
        116,
        36,
        108,
        68,
        80
      ],
      "accounts": [
        {
          "name": "buyer",
          "writable": true,
          "signer": true
        },
        {
          "name": "seller",
          "docs": [
            "Виконавець підписує угоду разом із замовником: він приймає не гроші, а",
            "політику розгляду, за якою його ж і судитимуть."
          ],
          "signer": true
        },
        {
          "name": "mint"
        },
        {
          "name": "integrator",
          "docs": [
            "Політика арбітражу, на яку погоджуються обидві сторони. Тип із",
            "VerdictMesh, тож Anchor звіряє й власника акаунта — підсунути сюди",
            "вигаданий `Integrator` нічим."
          ]
        },
        {
          "name": "config",
          "docs": [
            "Глобальний акаунт протоколу — потрібен рівно заради `settlement_mint`.",
            "Seed-ів звідси не виводимо: `Config` створюється один раз і за фіксованою",
            "адресою (`initialize`), тож акаунта цього типу, який належить VerdictMesh",
            "і при цьому не є тим самим, не існує. Перевірку типом Anchor робить сам."
          ]
        },
        {
          "name": "settlementMint",
          "docs": [
            "Актив застави — розрахунковий актив протоколу. Може бути тим самим, що й",
            "актив угоди, і це не заважає: каси різні."
          ]
        },
        {
          "name": "escrow",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  115,
                  99,
                  114,
                  111,
                  119
                ]
              },
              {
                "kind": "account",
                "path": "buyer"
              },
              {
                "kind": "arg",
                "path": "dealId"
              }
            ]
          }
        },
        {
          "name": "buyerTokens",
          "writable": true
        },
        {
          "name": "vault",
          "docs": [
            "Каса угоди. Авторитет — сам `Escrow`: приватного ключа до нього не існує,",
            "тож кошти виходять лише тим шляхом, який програма підписала сама."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  115,
                  99,
                  114,
                  111,
                  119,
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
                "path": "escrow"
              }
            ]
          }
        },
        {
          "name": "buyerBondTokens",
          "writable": true
        },
        {
          "name": "sellerBondTokens",
          "writable": true
        },
        {
          "name": "bondVault",
          "docs": [
            "Каса застав — окрема від каси угоди, бо активи різні й доля в них різна:",
            "предмет угоди дістається одній стороні, застава розходиться між обома."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  98,
                  111,
                  110,
                  100,
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
                "path": "escrow"
              }
            ]
          }
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "settlementTokenProgram",
          "docs": [
            "Програма токена **розрахункового** активу. Окремим акаунтом, бо актив",
            "угоди й актив протоколу можуть жити в різних програмах токена — класична",
            "`Tokenkeg` і Token-2022 не той самий `program_id`. Коли вони збігаються,",
            "клієнт передає один акаунт двічі."
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "dealId",
          "type": "u64"
        },
        {
          "name": "milestones",
          "type": {
            "vec": "u64"
          }
        }
      ]
    },
    {
      "name": "disputeMilestone",
      "docs": [
        "Відкриває спір над віхою одним CPI у VerdictMesh, підписуючи його",
        "власним PDA. Уся інтеграція — тут; вердикт звідси витягнуть, а не",
        "проштовхнуть сюди. Див. `instructions::escrow`."
      ],
      "discriminator": [
        199,
        209,
        70,
        146,
        136,
        43,
        179,
        41
      ],
      "accounts": [
        {
          "name": "claimant",
          "writable": true,
          "signer": true
        },
        {
          "name": "escrow",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  115,
                  99,
                  114,
                  111,
                  119
                ]
              },
              {
                "kind": "account",
                "path": "escrow.buyer",
                "account": "escrow"
              },
              {
                "kind": "account",
                "path": "escrow.deal_id",
                "account": "escrow"
              }
            ]
          }
        },
        {
          "name": "integrator",
          "docs": [
            "сама політика, під якою укладалась угода."
          ],
          "writable": true
        },
        {
          "name": "config"
        },
        {
          "name": "settlementMint"
        },
        {
          "name": "dispute",
          "writable": true
        },
        {
          "name": "claimantTokens",
          "writable": true
        },
        {
          "name": "disputeVault",
          "writable": true
        },
        {
          "name": "verdictMeshProgram",
          "address": "8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq"
        },
        {
          "name": "tokenProgram",
          "docs": [
            "Програма токена **розрахункового** активу, а не активу угоди: тут",
            "рухається лише депозит."
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "milestone",
          "type": "u8"
        }
      ]
    },
    {
      "name": "releaseMilestone",
      "docs": [
        "Закриває віху без спору: замовник віддає свої гроші добровільно, і",
        "більше ніхто цього зробити не може. Застава віхи повертається обом",
        "сторонам разом із нею — див. `instructions::escrow`."
      ],
      "discriminator": [
        56,
        2,
        199,
        164,
        184,
        108,
        167,
        222
      ],
      "accounts": [
        {
          "name": "buyer",
          "signer": true,
          "relations": [
            "escrow"
          ]
        },
        {
          "name": "escrow",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  115,
                  99,
                  114,
                  111,
                  119
                ]
              },
              {
                "kind": "account",
                "path": "escrow.buyer",
                "account": "escrow"
              },
              {
                "kind": "account",
                "path": "escrow.deal_id",
                "account": "escrow"
              }
            ]
          }
        },
        {
          "name": "mint"
        },
        {
          "name": "sellerTokens",
          "writable": true
        },
        {
          "name": "vault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  115,
                  99,
                  114,
                  111,
                  119,
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
                "path": "escrow"
              }
            ]
          }
        },
        {
          "name": "settlementMint"
        },
        {
          "name": "buyerBondTokens",
          "docs": [
            "Застава повертається обом сторонам, тож обидва акаунти передаються",
            "завжди й обидва прив'язані до ролей з угоди."
          ],
          "writable": true
        },
        {
          "name": "sellerBondTokens",
          "writable": true
        },
        {
          "name": "bondVault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  98,
                  111,
                  110,
                  100,
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
                "path": "escrow"
              }
            ]
          }
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "settlementTokenProgram"
        }
      ],
      "args": [
        {
          "name": "milestone",
          "type": "u8"
        }
      ]
    },
    {
      "name": "settleMilestone",
      "docs": [
        "Виконує вердикт над віхою: читає `Dispute`, робить чотири перевірки і",
        "розподіляє кошти сам. Нічия інструкція без жодного підпису — див.",
        "`instructions::settle`."
      ],
      "discriminator": [
        0,
        239,
        52,
        170,
        175,
        209,
        205,
        224
      ],
      "accounts": [
        {
          "name": "escrow",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  115,
                  99,
                  114,
                  111,
                  119
                ]
              },
              {
                "kind": "account",
                "path": "escrow.buyer",
                "account": "escrow"
              },
              {
                "kind": "account",
                "path": "escrow.deal_id",
                "account": "escrow"
              }
            ]
          }
        },
        {
          "name": "dispute",
          "docs": [
            "Розгляд, чий вердикт виконується. Тип із VerdictMesh, тож Anchor звіряє",
            "власника акаунта: виписати собі вердикт, виклавши `Dispute` цією ж",
            "програмою, нічим. **Read-only** — ескроу нічого в чужому стані не міняє."
          ]
        },
        {
          "name": "mint"
        },
        {
          "name": "buyerTokens",
          "docs": [
            "Обидва токен-акаунти передаються завжди, і кожен прив'язаний до свого",
            "власника з угоди. Передавати лише акаунт переможця означало б дати тому,",
            "хто викликає дозвільну інструкцію, вибирати отримувача."
          ],
          "writable": true
        },
        {
          "name": "sellerTokens",
          "writable": true
        },
        {
          "name": "vault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  115,
                  99,
                  114,
                  111,
                  119,
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
                "path": "escrow"
              }
            ]
          }
        },
        {
          "name": "settlementMint"
        },
        {
          "name": "buyerBondTokens",
          "docs": [
            "Застава розходиться між обома сторонами, і навіть коли одна з часток",
            "нульова, обидва акаунти передаються — інакше той, хто викликає дозвільну",
            "інструкцію, вибирав би, кому дістанеться відшкодування."
          ],
          "writable": true
        },
        {
          "name": "sellerBondTokens",
          "writable": true
        },
        {
          "name": "bondVault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  98,
                  111,
                  110,
                  100,
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
                "path": "escrow"
              }
            ]
          }
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "settlementTokenProgram"
        }
      ],
      "args": [
        {
          "name": "milestone",
          "type": "u8"
        }
      ]
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
      "name": "escrow",
      "discriminator": [
        31,
        213,
        123,
        187,
        186,
        22,
        218,
        155
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
    }
  ],
  "events": [
    {
      "name": "escrowOpened",
      "discriminator": [
        201,
        42,
        96,
        8,
        123,
        9,
        212,
        164
      ]
    },
    {
      "name": "milestoneDisputed",
      "discriminator": [
        83,
        106,
        229,
        228,
        159,
        61,
        122,
        16
      ]
    },
    {
      "name": "milestoneReleased",
      "discriminator": [
        49,
        225,
        91,
        223,
        34,
        165,
        109,
        181
      ]
    },
    {
      "name": "milestoneSettled",
      "discriminator": [
        14,
        243,
        90,
        90,
        207,
        201,
        235,
        116
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "invalidParties",
      "msg": "A deal needs two different parties"
    },
    {
      "code": 6001,
      "name": "invalidMilestones",
      "msg": "Milestone amounts are empty, too many, or worth nothing"
    },
    {
      "code": 6002,
      "name": "unknownMilestone",
      "msg": "There is no milestone with this number in the deal"
    },
    {
      "code": 6003,
      "name": "milestoneNotPending",
      "msg": "The milestone is not open: it is already settled or under dispute"
    },
    {
      "code": 6004,
      "name": "notAParty",
      "msg": "Only the buyer or the seller of this deal may act on it"
    },
    {
      "code": 6005,
      "name": "wrongIntegrator",
      "msg": "The dispute must be opened under the policy the deal was created with"
    },
    {
      "code": 6006,
      "name": "wrongArbitrationProgram",
      "msg": "The integrator record points at another escrow program"
    },
    {
      "code": 6007,
      "name": "overflow",
      "msg": "Arithmetic overflow"
    },
    {
      "code": 6008,
      "name": "notOurDispute",
      "msg": "This dispute was opened over another escrow"
    },
    {
      "code": 6009,
      "name": "verdictPending",
      "msg": "The dispute has no verdict yet"
    },
    {
      "code": 6010,
      "name": "verdictUnderAppeal",
      "msg": "The verdict is frozen while the dispute is under appeal"
    },
    {
      "code": 6011,
      "name": "appealWindowOpen",
      "msg": "The appeal window has not closed yet"
    },
    {
      "code": 6012,
      "name": "milestoneNotUnderThisDispute",
      "msg": "This milestone is not under the dispute that was brought"
    },
    {
      "code": 6013,
      "name": "verdictNamesAStranger",
      "msg": "The verdict names a party that is not in this deal"
    },
    {
      "code": 6014,
      "name": "wrongSettlementMint",
      "msg": "The review bond must be held in the settlement asset of the protocol"
    }
  ],
  "types": [
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
      "name": "escrow",
      "docs": [
        "Угода з віхами: замовник замикає всю суму наперед, віхи закриваються по",
        "одній. Спір іде **над віхою**, а не над угодою — інакше незгода щодо",
        "останнього кроку ставила б під сумнів усе, що вже прийнято."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "buyer",
            "docs": [
              "Хто платить і хто виконує. Ролі фіксовані на весь час угоди: позиція",
              "сторони в спорі виводиться з ролі, а не заявляється (див. `claims`)."
            ],
            "type": "pubkey"
          },
          {
            "name": "seller",
            "type": "pubkey"
          },
          {
            "name": "mint",
            "docs": [
              "Актив угоди. Може не збігатися з розрахунковим активом протоколу —",
              "депозит за розгляд однаково береться в тому (`FR-011a`)."
            ],
            "type": "pubkey"
          },
          {
            "name": "settlementMint",
            "docs": [
              "Актив, у якому лежить застава за розгляд. Це `Config.settlement_mint`",
              "протоколу, звірений при укладанні: заставою відшкодовується **депозит**,",
              "і застава в іншому активі не відшкодувала б нічого (`FR-026e`).",
              "Зберігається тут, щоб закриття віхи не мусило читати чужий `Config`",
              "заради одного ключа."
            ],
            "type": "pubkey"
          },
          {
            "name": "integrator",
            "docs": [
              "PDA `Integrator` у VerdictMesh — тобто **політика**, на яку сторони",
              "погодились, створюючи угоду. Не адреса програми арбітражу: політику",
              "підмінити легше, ніж програму, а розглядають за нею."
            ],
            "type": "pubkey"
          },
          {
            "name": "bond",
            "docs": [
              "Застава за розгляд однієї віхи з одного боку — `FR-026e`. Знімок",
              "`Policy.deposit` на момент укладання, а не посилання на політику:",
              "застава, що росла б разом із політикою, вимагала б доносити кошти в уже",
              "підписану угоду. Розбіжність зі знімком у самому спорі й дає `FR-026f`."
            ],
            "type": "u64"
          },
          {
            "name": "dealId",
            "docs": [
              "Номер угоди в замовника. Задає клієнт, а не лічильник: акаунта, який",
              "його вів би, тут немає, а повтор однаково не пройде — за тією ж адресою",
              "уже щось лежить."
            ],
            "type": "u64"
          },
          {
            "name": "milestones",
            "docs": [
              "`max_len(0)` — не «угода без віх». Довжина відома лише в момент",
              "створення, тож `InitSpace` рахує тут 4-байтовий префікс, а решту додає",
              "`Escrow::space`. Так фіксовану частину все одно рахує макрос, і нове",
              "поле не може мовчки випасти з розрахунку."
            ],
            "type": {
              "vec": {
                "defined": {
                  "name": "milestone"
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
      "name": "escrowOpened",
      "docs": [
        "Події тут — не дзеркало VerdictMesh, а те, з чого fact-finding (T028)",
        "відновлює **предмет** спору: угоду, її віхи й те, що з ними вже сталося.",
        "Ончейн-факт із посиланням на підпис транзакції (`FR-016`) береться саме",
        "звідси."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "escrow",
            "type": "pubkey"
          },
          {
            "name": "buyer",
            "type": "pubkey"
          },
          {
            "name": "seller",
            "type": "pubkey"
          },
          {
            "name": "mint",
            "type": "pubkey"
          },
          {
            "name": "integrator",
            "type": "pubkey"
          },
          {
            "name": "total",
            "type": "u64"
          },
          {
            "name": "milestones",
            "type": "u8"
          },
          {
            "name": "bond",
            "docs": [
              "Застава за розгляд однієї віхи з одного боку — `FR-026e`. Умови угоди",
              "видно з однієї події цілком: замкнено `total` предмета плюс",
              "`bond × milestones` з кожного боку."
            ],
            "type": "u64"
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
      "name": "milestone",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "state",
            "type": {
              "defined": {
                "name": "milestoneState"
              }
            }
          }
        ]
      }
    },
    {
      "name": "milestoneDisputed",
      "docs": [
        "Віха пішла на розгляд. `claimant` тут — не зайве дублювання спору: саме за",
        "ним виконання вердикту (T023) розуміє, кому дістається `Verdict::Claimant`,",
        "і спостерігач мусить бачити той самий зв'язок."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "escrow",
            "type": "pubkey"
          },
          {
            "name": "milestone",
            "type": "u8"
          },
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "claimant",
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
      "name": "milestoneReleased",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "escrow",
            "type": "pubkey"
          },
          {
            "name": "milestone",
            "type": "u8"
          },
          {
            "name": "amount",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "milestoneSettled",
      "docs": [
        "Вердикт виконано — `FR-012`. `winner` порожній рівно за статус-кво: розгляд",
        "закінчився, кошти не рухались, віха повернулась туди, звідки її взяли.",
        "Самого вердикту тут немає навмисно — його вже оголосив VerdictMesh",
        "(`DisputeTallied`), і другий запис того самого факту рано чи пізно",
        "розійшовся б з першим."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "escrow",
            "type": "pubkey"
          },
          {
            "name": "milestone",
            "type": "u8"
          },
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "winner",
            "type": {
              "option": "pubkey"
            }
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "reimbursed",
            "docs": [
              "Скільки застави програвшої сторони пішло на відшкодування депозиту",
              "ініціаторові — `FR-026a`. Нуль означає, що розгляд оплатив сам ініціатор:",
              "або він програв, або вердикт лишив сторони там, де вони були. Без цього",
              "поля відповідь на «хто зрештою поніс вартість розгляду» довелось би",
              "збирати з двох програм і трьох переказів."
            ],
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "milestoneState",
      "docs": [
        "Що сталося з віхою.",
        "",
        "**`Disputed` носить адресу свого розгляду**, а не прапорець. Без неї",
        "виконання вердикту довелось би прив'язувати до віхи за здогадкою «та, що в",
        "спорі», і будь-який **інший** спір цього ж ескроу — навіть давно",
        "фіналізований — зійшовся б за `escrow_ref` і розпорядився б чужою віхою.",
        "Саме це `FR-013` і забороняє: адреса в стані робить повторне виконання",
        "неможливим, бо після розрахунку її там уже немає."
      ],
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "pending"
          },
          {
            "name": "disputed",
            "fields": [
              {
                "name": "dispute",
                "type": "pubkey"
              }
            ]
          },
          {
            "name": "released"
          },
          {
            "name": "refunded"
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
    }
  ]
};
