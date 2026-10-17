# Bot lethal attack assessment

Codex contribution alongside Claude's existing bot work. Attack selection used
`blocker_count * biggest_attacker_damage` to estimate prevention, which could
count the same large attacker repeatedly and miss lethal attacks. Selection and
destination planning now share a conservative estimate: subtract the largest
distinct blockable attackers, at most one per potential blocker. Flying attackers
that no blocker can reach contribute guaranteed damage.

Regressions cover a 10/1 plus two 2/1s versus two 20/20 blockers at two life
(attack) and three life (hold), plus lethal flying damage alongside a ground
attacker. `cargo test -p mtg-policy`: 44 passed.

This remains a heuristic: it does not optimize blocker matching, trample damage,
or multiplayer defender selection. Existing unrelated working changes retained.
