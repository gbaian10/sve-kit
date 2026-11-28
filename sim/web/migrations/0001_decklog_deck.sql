-- DeckLog decks looked up through the Worker. A published code's cards never change, so a row is
-- written once and served from here afterwards. Only what deck building needs is kept: no deck
-- title, memo, comment or owner from DeckLog's answer.
CREATE TABLE decklog_deck (
  region TEXT NOT NULL CHECK (region IN ('jp', 'en')),
  code TEXT NOT NULL,
  -- DeckLog's construction code (N, X, G) and the class or title it shows; NULL when absent.
  format TEXT,
  class_name TEXT,
  -- JSON {"leader":[...],"main":[...],"evolve":[...]} of {"cardNo","count"} rows.
  cards TEXT NOT NULL,
  fetched_at TEXT NOT NULL,
  PRIMARY KEY (region, code)
) STRICT, WITHOUT ROWID;
