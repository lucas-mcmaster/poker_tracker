"""Write `tests/fixtures/equity/eval7_spots.txt`: random and tie-provoking equity spots with
their exact equities from `eval7`, for the crate's `exact_equity` regression test.

The spots are fixed by the seed, so the file is reproducible. Every equity is computed here by
enumerating the board completions with `eval7` (independent of the crate) and is written as a
reduced fraction.

    python3 -m venv /tmp/oracle-venv && /tmp/oracle-venv/bin/pip install eval7
    /tmp/oracle-venv/bin/python -I tools/oracles/equity_spots.py tests/fixtures/equity/eval7_spots.txt

Line format: `<hole>|<hole>|...;<board>;<num>/<den> <num>/<den> ...`, cards as `Ah`, holes and
board space-separated, the board empty for preflop.
"""
import itertools
import random
import sys
from fractions import Fraction

import eval7

RANKS = "23456789TJQKA"
SUITS = "cdhs"
DECK = [r + s for r in RANKS for s in SUITS]
SEED = 20261103


def tie_board(rng):
    """A river board built to cause ties and kicker battles."""
    k = rng.randrange(6)
    if k == 0:  # straight on board, wheel included
        top = rng.randrange(3, 13)
        ranks = ["5", "4", "3", "2", "A"] if top == 3 else [RANKS[top - i] for i in range(5)]
        return [r + rng.choice(SUITS) for r in ranks]
    if k == 1:  # four or five of one suit
        s = rng.choice(SUITS)
        cards = [r + s for r in rng.sample(RANKS, rng.choice([4, 5]))]
    elif k == 2:  # quads on board
        r = rng.choice(RANKS)
        cards = [r + s for s in SUITS]
    elif k == 3:  # full house on board
        a, b = rng.sample(RANKS, 2)
        return [a + s for s in rng.sample(SUITS, 3)] + [b + s for s in rng.sample(SUITS, 2)]
    elif k == 4:  # two pair on board
        a, b, c = rng.sample(RANKS, 3)
        return ([a + s for s in rng.sample(SUITS, 2)] + [b + s for s in rng.sample(SUITS, 2)]
                + [c + rng.choice(SUITS)])
    else:  # trips on board
        a, b, c = rng.sample(RANKS, 3)
        return [a + s for s in rng.sample(SUITS, 3)] + [b + rng.choice(SUITS), c + rng.choice(SUITS)]
    while len(cards) < 5:
        c = rng.choice(DECK)
        if c not in cards:
            cards.append(c)
    return cards


def spots(rng):
    def random_spot(n, b):
        deck = DECK[:]
        rng.shuffle(deck)
        return [deck[b + 2 * i: b + 2 * i + 2] for i in range(n)], deck[:b]

    for _ in range(1000):
        yield random_spot(rng.randrange(2, 7), 5)
    for _ in range(1000):
        board = tie_board(rng)
        deck = [c for c in DECK if c not in board]
        rng.shuffle(deck)
        n = rng.randrange(2, 7)
        yield [deck[2 * i: 2 * i + 2] for i in range(n)], board
    for _ in range(100):
        yield random_spot(rng.randrange(2, 5), 4)
    for _ in range(30):
        yield random_spot(rng.randrange(2, 5), 3)


def equity(holes, board):
    used = set(board) | {c for h in holes for c in h}
    rest = [eval7.Card(c) for c in DECK if c not in used]
    hs = [[eval7.Card(c) for c in h] for h in holes]
    b = [eval7.Card(c) for c in board]
    wins = [Fraction(0)] * len(holes)
    total = 0
    for run in itertools.combinations(rest, 5 - len(board)):
        scores = [eval7.evaluate(b + list(run) + h) for h in hs]
        top = max(scores)
        winners = [i for i, s in enumerate(scores) if s == top]
        for i in winners:
            wins[i] += Fraction(1, len(winners))
        total += 1
    return [w / total for w in wins]


def main(out):
    rng = random.Random(SEED)
    lines = []
    for holes, board in spots(rng):
        eq = equity(holes, board)
        lines.append("|".join(" ".join(h) for h in holes) + ";" + " ".join(board) + ";"
                     + " ".join(f"{e.numerator}/{e.denominator}" for e in eq))
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {len(lines)} spots to {out}")


if __name__ == "__main__":
    main(sys.argv[1])
