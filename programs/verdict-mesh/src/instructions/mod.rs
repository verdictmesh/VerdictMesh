pub mod commit_vote;
pub mod initialize;
pub mod open_dispute;
pub mod register_integrator;
pub mod reveal_vote;
pub mod select_panel;
pub mod stake;
pub mod unstake;

// Глоб потрібен макросу `#[program]`: згенеровані `#[derive(Accounts)]` модулі
// він шукає в корені крейта. Тому хендлер кожної інструкції — асоційована
// функція її контексту, а не вільна `fn`: вільні функції потрапили б у глоб і
// зіткнулися б між собою або з іменами, які генерує сам `#[program]`.
pub use commit_vote::*;
pub use initialize::*;
pub use open_dispute::*;
pub use register_integrator::*;
pub use reveal_vote::*;
pub use select_panel::*;
pub use stake::*;
pub use unstake::*;
