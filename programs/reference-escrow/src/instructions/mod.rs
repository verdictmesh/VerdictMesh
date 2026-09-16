pub mod escrow;
pub mod settle;

// Глоб потрібен макросу `#[program]`: згенеровані `#[derive(Accounts)]` модулі
// він шукає в корені крейта. Тому хендлер кожної інструкції — асоційована
// функція її контексту, а не вільна `fn`.
pub use escrow::*;
pub use settle::*;
