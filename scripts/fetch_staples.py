#!/usr/bin/env python3
"""Fetch ranked Commander staple lists for coverage work.

Writes, one card name per line in priority order (a tab and a score may follow):

* ``cedh.txt`` — cEDH staples from EDHTop16's public GraphQL API, by tournament play
  rate over the last year;
* ``edh.txt`` — EDHREC's top cards (overall, then per color, colorless, lands and
  multicolor), deduplicated in that order.

Check them against the compiler with ``mtg-cards check <file>``. Lists change over time;
record the date fetched alongside any numbers quoted from them.
"""
import argparse
import json
from pathlib import Path
import urllib.request

EDHTOP16 = "https://edhtop16.com/api/graphql"
EDHREC = "https://json.edhrec.com/pages/top/{}.json"
EDHREC_PAGES = ["year", "white", "blue", "black", "red", "green", "colorless", "lands", "multicolor"]


def fetch(url, data=None):
    headers = {"content-type": "application/json", "user-agent": "mtgo-rs-staples/1"}
    request = urllib.request.Request(url, data=data, headers=headers)
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)


def cedh():
    query = json.dumps({"query": "{ staples { name playRateLastYear } }"}).encode()
    staples = fetch(EDHTOP16, query)["data"]["staples"]
    staples.sort(key=lambda c: -(c["playRateLastYear"] or 0))
    return [f"{c['name']}\t{c['playRateLastYear']}" for c in staples]


def edh():
    names = []
    for page in EDHREC_PAGES:
        lists = fetch(EDHREC.format(page))["container"]["json_dict"]["cardlists"]
        for cardlist in lists:
            for card in cardlist.get("cardviews", []):
                if card["name"] not in names:
                    names.append(card["name"])
    return names


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="directory to write into")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for name, rows in [("cedh.txt", cedh()), ("edh.txt", edh())]:
        (args.output / name).write_text("\n".join(rows) + "\n")
        print(f"{name}: {len(rows)} cards")


if __name__ == "__main__":
    main()
