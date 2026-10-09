# Release publication failure — 2026-10-09

User reported failing GitHub jobs and authorized releasing/pushing the candidate.
The latest remote commit is 5f35aacd9eb1af644eab8114f99fd39fc091cf52.
[Verification](https://github.com/Lvcky-gg/mtgo_rs/actions/runs/37964669009)
and [Pages](https://github.com/Lvcky-gg/mtgo_rs/actions/runs/37964669087)
succeeded. Both recent Release runs built all four platforms and passed their
Linux test job, but failed at Create or update the release for this commit:
[latest](https://github.com/Lvcky-gg/mtgo_rs/actions/runs/37964669028/job/113939052891),
[previous](https://github.com/Lvcky-gg/mtgo_rs/actions/runs/37867447842/job/113619567001).

The public artifacts API confirms four installer artifacts plus
release-verification-37964669028-1. The publish job downloaded release-*, which
also selected that verification artifact, then passed dist/* to gh release.
The evidence archive includes campaign/, so the CLI attempted to upload a
directory alongside the installers. This defect reproduces against the actual
old workflow shell with a local CLI stand-in, on both first publication and
retry. Authenticated raw job logs were unavailable, so this is a reproduced
workflow defect consistent with the observed failing step, not a quotation from
GitHub's error log.

The workflow now downloads only the four named platform artifacts and uses an
explicit list of six installer/archive files plus SHA256SUMS.txt for both create
and update. Existing checksum checks and publication gating remain in force.
Regression tests execute the actual publication shell with installer fixtures,
extra verification files and a campaign directory; both paths now pass.

Python automation: 81 tests run, 80 passed and one Flatpak test skipped because
Flatpak is not installed; /tmp/mtgo-github-release-python.log. The old-workflow
regression records two expected failures, both cannot upload a directory;
/tmp/mtgo-github-publish-before.log. Previous current Rust source validation was
2,154 passed/44 ignored with Clippy clean; no Rust source changed in this fix.
Fresh PR campaign: all 35 checks passed;
/tmp/mtgo-github-release-campaign/report.json. The ordinary current-source gate
passed; /tmp/mtgo-github-release-gate.json. Source fingerprint:
0824deccce317dd542a12b4582537824f84e8b9eb665864b73023cd8ad90dfdc.
The fix and verified alpha changes are ready to commit and push. GitHub rejected
the stored credential during log retrieval and the SSH key during authentication;
the actual requested push is attempted after committing. Independent closed-alpha assessments remain pending; the user
has authorized an ordinary development release, not invented review outcomes.
