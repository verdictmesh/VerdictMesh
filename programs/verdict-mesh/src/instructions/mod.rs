pub mod initialize;
pub mod open_dispute;
pub mod register_integrator;

// Глоб потрібен макросу `#[program]`: згенеровані `#[derive(Accounts)]` модулі
// він шукає в корені крейта. Тому хендлер кожної інструкції — асоційована
// функція її контексту, а не вільна `fn`: вільні функції потрапили б у глоб і
// зіткнулися б між собою або з іменами, які генерує сам `#[program]`.
pub use initialize::*;
pub use open_dispute::*;
pub use register_integrator::*;
