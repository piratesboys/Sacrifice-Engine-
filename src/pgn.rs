use crate::board::Board;
use chess::{ChessMove, Piece};
use std::collections::HashMap;
use std::fs;
use std::str::FromStr;

#[derive(Clone, Debug, Default)]
pub struct StyleBook {
    games_count: u64,
    positions_count: u64,
    moves_count: u64,
    sacrifices_count: u64,
    checks_count: u64,
    captures_count: u64,
    invalid_tokens_count: u64,

    positions: HashMap<u64, u32>,
    moves: HashMap<u64, HashMap<ChessMove, u32>>,
    tactical_positions: HashMap<u64, u32>,
}

impl StyleBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn load_file(&mut self, path: &str) -> Result<(), String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("impossible de lire le PGN {path}: {e}"))?;

        self.load_pgn(&text);
        Ok(())
    }

    /// Charge les parties PGN et ajoute leurs statistiques au livre.
    pub fn load_pgn(&mut self, text: &str) {
        let mut games = Vec::<String>::new();
        let mut current = String::new();

        for line in text.lines() {
            let trimmed = line.trim();

            // Une nouvelle balise Event marque généralement une nouvelle partie.
            if trimmed.starts_with("[Event ") && !current.trim().is_empty() {
                games.push(std::mem::take(&mut current));
            }

            current.push_str(line);
            current.push('\n');
        }

        if !current.trim().is_empty() {
            games.push(current);
        }

        if games.is_empty() && !text.trim().is_empty() {
            games.push(text.to_string());
        }

        for game in games {
            if self.parse_game(&game) {
                self.games_count += 1;
            }
        }
    }

    fn parse_game(&mut self, game: &str) -> bool {
        let mut movetext = String::new();

        // Ignore les lignes d'en-tête : [Event "..."], [White "..."], etc.
        for line in game.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                continue;
            }

            movetext.push_str(line);
            movetext.push('\n');
        }

        let tokens = tokenize_movetext(&movetext);
        let mut board = Board::startpos();
        let mut parsed_any = false;

        // Permet de reconnaître certains sacrifices matériels simples :
        // un coup non capturant suivi immédiatement de la capture de la pièce
        // qui vient d'être déplacée.
        let mut previous_move: Option<(chess::Square, Piece)> = None;

        for token in tokens {
            if is_result(&token) {
                break;
            }

            let san = clean_san(&token);

            if san.is_empty()
                || is_result(&san)
                || san.starts_with('$')
                || san.eq_ignore_ascii_case("e.p.")
                || san.eq_ignore_ascii_case("ep")
            {
                continue;
            }

            let Some(mv) = san_to_move(&board, &san) else {
                self.invalid_tokens_count += 1;
                continue;
            };

            let moving_piece = board.piece_on(mv.get_source());
            let capture = is_capture(&board, mv);

            // Si l'adversaire capture immédiatement la pièce déplacée au coup
            // précédent, on compte cela comme un sacrifice matériel potentiel.
            if capture {
                if let Some((previous_square, previous_piece)) = previous_move {
                    if mv.get_dest() == previous_square
                        && moving_piece.is_some()
                        && piece_value(previous_piece) > 100
                    {
                        self.sacrifices_count += 1;
                    }
                }
            }

            self.process_move(&board, mv);

            let next_board = board.make_move_new(mv);

            previous_move = moving_piece.map(|piece| (mv.get_dest(), piece));
            board = next_board;
            parsed_any = true;
        }

        parsed_any
    }

    fn process_move(&mut self, board: &Board, mv: ChessMove) {
        let hash = board.hash();

        *self.positions.entry(hash).or_insert(0) += 1;

        *self.moves.entry(hash).or_default().entry(mv).or_insert(0) += 1;

        self.positions_count += 1;
        self.moves_count += 1;

        let capture = is_capture(board, mv);

        if capture {
            self.captures_count += 1;
        }

        let next_board = board.make_move_new(mv);
        let gives_check = next_board.in_check();

        if gives_check {
            self.checks_count += 1;
        }

        if capture || gives_check || mv.get_promotion().is_some() {
            *self.tactical_positions.entry(hash).or_insert(0) += 1;
        }
    }

    /// Renvoie le coup le plus fréquemment joué dans cette position exacte.
    pub fn preferred_move(&self, board: &Board) -> Option<ChessMove> {
        self.moves
            .get(&board.hash())?
            .iter()
            .max_by(|(move_a, count_a), (move_b, count_b)| {
                count_a
                    .cmp(count_b)
                    .then_with(|| move_b.to_string().cmp(&move_a.to_string()))
            })
            .map(|(mv, _)| *mv)
    }

    /// Score de préférence historique borné.
    /// Utilisé pour l'ordre des coups, jamais comme bonus d'évaluation.
    pub fn move_style_score(&self, board: &Board, mv: ChessMove) -> i32 {
        let Some(moves) = self.moves.get(&board.hash()) else {
            return 0;
        };

        let Some(&count) = moves.get(&mv) else {
            return 0;
        };

        let total: u32 = moves.values().copied().sum();

        if total == 0 {
            return 0;
        }

        let frequency = (u64::from(count) * 700 / u64::from(total)) as i32;

        let repetition = (count.min(30) as i32) * 10;

        (frequency + repetition).min(1_000)
    }

    /// Indique si une position connue présente un historique tactique.
    pub fn position_bias(&self, board: &Board) -> i32 {
        let hash = board.hash();

        if !self.positions.contains_key(&hash) {
            return 0;
        }

        i32::from(self.tactical_positions.contains_key(&hash))
    }

    pub fn position_move_count(&self, board: &Board) -> u32 {
        self.moves
            .get(&board.hash())
            .map(|moves| moves.values().copied().sum())
            .unwrap_or(0)
    }

    pub fn games(&self) -> u64 {
        self.games_count
    }

    pub fn positions(&self) -> u64 {
        self.positions_count
    }

    pub fn moves(&self) -> u64 {
        self.moves_count
    }

    pub fn sacrifices(&self) -> u64 {
        self.sacrifices_count
    }

    pub fn checks(&self) -> u64 {
        self.checks_count
    }

    pub fn captures(&self) -> u64 {
        self.captures_count
    }

    pub fn invalid_tokens(&self) -> u64 {
        self.invalid_tokens_count
    }
}

/// Supprime les commentaires, les variantes et les commentaires de ligne.
fn tokenize_movetext(text: &str) -> Vec<String> {
    let mut output = String::new();
    let mut brace_depth = 0usize;
    let mut variation_depth = 0usize;
    let mut line_comment = false;

    for ch in text.chars() {
        if line_comment {
            if ch == '\n' {
                line_comment = false;
                output.push(' ');
            }
            continue;
        }

        match ch {
            ';' if brace_depth == 0 && variation_depth == 0 => {
                line_comment = true;
            }

            '{' if variation_depth == 0 => {
                brace_depth += 1;
            }

            '}' if brace_depth > 0 => {
                brace_depth -= 1;
            }

            '(' if brace_depth == 0 => {
                variation_depth += 1;
            }

            ')' if brace_depth == 0 && variation_depth > 0 => {
                variation_depth -= 1;
            }

            _ if brace_depth == 0 && variation_depth == 0 => {
                output.push(ch);
            }

            _ => {}
        }
    }

    output.split_whitespace().map(str::to_string).collect()
}

/// Normalise les notations comme 1.e4, 1...e5, Nf3!? et 23.Qh5+.
fn clean_san(token: &str) -> String {
    let mut san = token.trim().to_string();

    // Ignore les NAG intégrés au token, par exemple e4$1.
    if let Some(index) = san.find('$') {
        san.truncate(index);
    }

    // Retire le numéro du coup sans supprimer les chiffres d'une notation UCI.
    let digit_count = san.chars().take_while(|ch| ch.is_ascii_digit()).count();

    if digit_count > 0 && san.as_bytes().get(digit_count) == Some(&b'.') {
        san.drain(..digit_count);
        san = san.trim_start_matches('.').to_string();
    }

    san = san.trim().to_string();

    // Retire les annotations éditoriales finales : !, ?, !!, ?!, etc.
    while matches!(san.chars().last(), Some('!') | Some('?')) {
        san.pop();
    }

    // Certains PGN utilisent des zéros pour le roque.
    if san.starts_with("0-0-0") {
        san = san.replacen("0-0-0", "O-O-O", 1);
    } else if san.starts_with("0-0") {
        san = san.replacen("0-0", "O-O", 1);
    }

    san
}

fn is_result(token: &str) -> bool {
    matches!(token, "1-0" | "0-1" | "1/2-1/2" | "*")
}

fn san_to_move(board: &Board, san: &str) -> Option<ChessMove> {
    // Première tentative : notation SAN exacte.
    if let Ok(mv) = ChessMove::from_san(board.inner(), san) {
        if board.is_legal(mv) {
            return Some(mv);
        }
    }

    // Certains fichiers ajoutent un suffixe de check ou de mat que le parseur
    // peut ne pas accepter. On essaie alors sans ce suffixe.
    let without_check = san.trim_end_matches(['+', '#']);

    if without_check != san {
        if let Ok(mv) = ChessMove::from_san(board.inner(), without_check) {
            if board.is_legal(mv) {
                return Some(mv);
            }
        }
    }

    // Repli pour les coups exprimés directement en UCI, comme e2e4 ou e7e8q.
    ChessMove::from_str(san)
        .ok()
        .filter(|mv| board.is_legal(*mv))
}

fn is_capture(board: &Board, mv: ChessMove) -> bool {
    if board.piece_on(mv.get_dest()).is_some() {
        return true;
    }

    // En passant : le pion capturé n'occupe pas la case de destination.
    board.piece_on(mv.get_source()) == Some(Piece::Pawn)
        && mv.get_source().get_file() != mv.get_dest().get_file()
}

fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => 100,
        Piece::Knight => 320,
        Piece::Bishop => 330,
        Piece::Rook => 500,
        Piece::Queen => 900,
        Piece::King => 20_000,
    }
}

pub fn move_to_uci(mv: ChessMove) -> String {
    mv.to_string()
}

#[cfg(test)]
mod tests {
    use super::clean_san;

    #[test]
    fn normalizes_zero_castling_and_move_numbers() {
        for (input, expected) in [
            ("0-0", "O-O"),
            ("0-0-0", "O-O-O"),
            ("0-0+!", "O-O+"),
            ("8.0-0", "O-O"),
            ("8...0-0-0", "O-O-O"),
            ("1.e4", "e4"),
            ("1...e5", "e5"),
            ("e2e4", "e2e4"),
        ] {
            assert_eq!(clean_san(input), expected, "input: {input}");
        }
    }
}
