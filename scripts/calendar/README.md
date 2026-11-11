# Event calendar loader

Builds the margin event calendar (`docs/risk/margin-model.md`, section 6.3) from official central-bank and statistics schedules, and loads it into the clearing canister through `add_calendar_events`.

No date is invented. Every event is read at run time from its publisher and carries the source URL. If a page no longer looks as expected, its parser fails loudly instead of guessing.

## Usage

```bash
# 1. Build: fetch every official source and write a reviewable calendar.json.
#    FRED_API_KEY is optional: without it US CPI and payrolls are skipped with a warning.
FRED_API_KEY=... node scripts/calendar/build.mjs --out calendar.json

# 2. Review calendar.json, then dry run against the canister (writes nothing).
scripts/calendar/load.sh --file calendar.json --network ic --identity risk-admin

# 3. Load.
scripts/calendar/load.sh --file calendar.json --network ic --identity risk-admin --apply
```

- `build.mjs` exits non-zero if any source fails. `--allow-partial` accepts a partial calendar; `--sources fomc,ecb` limits the run to some sources. A source whose key is missing (`FRED_API_KEY`) is reported as `skipped`, not failed, and the rest still builds.
- `load.sh` is idempotent: an event already on the canister (same time and label) is skipped, so it can be rerun after every refresh. To change an existing event's weight, remove it first with `remove_calendar_event`. It keeps at most 512 live events per underlying, as the canister enforces; events that ended more than a day ago are pruned by the canister and do not count.
- The identity must hold the risk administrator role, and every underlying must already be configured with `set_underlying_risk_params`.
- Past events are dropped at build time, and again at load time for anything less than 10 minutes away, because the canister rejects a whole batch if one event is already in the past. If a batch is still rejected for a past or duplicate event, `load.sh` re-reads the canister and retries once.
- Holidays can be loaded as events with a weight below 1 (the canister clamps each day at 0).

## Configuration

Everything that may need tuning lives in `config.json`:

- `weights`: provisional weight per event class (central-bank decision 3, US CPI 2, US payrolls 2). They are to be recalibrated from realized variance around past events, and must stay within the endpoint bounds `[0, 10]`.
- `underlyings`: the FX and crypto underlyings. Every FX underlying is quoted against USD, so it receives its own central bank's events plus the US events; crypto underlyings receive the US events.
- `sources`: URL, time zone and release time per source, with `time_basis` stating whether the time is published by the source or a documented convention.
- `unsupported`: publishers that cannot be read automatically today, and why.

Times are converted to UTC with the official time-zone database built into Node, so daylight saving is handled per publisher.

## Sources

| Source                                                                            | Underlyings | Time                                            |
| --------------------------------------------------------------------------------- | ----------- | ----------------------------------------------- |
| Federal Reserve FOMC calendar                                                     | all         | 14:00 New York, published                       |
| ECB Governing Council calendar                                                    | EUR         | 14:15 Frankfurt, published                      |
| Bank of England MPC dates                                                         | GBP         | 12:00 London, published                         |
| Bank of Japan MPM schedule                                                        | JPY         | 12:00 Tokyo, convention (no fixed release time) |
| RBA board meeting schedules                                                       | AUD         | 14:30 Sydney, published                         |
| SNB event schedule                                                                | CHF         | press-release time listed by the SNB            |
| Bank of Canada upcoming events                                                    | CAD         | 09:45 Ottawa, published                         |
| RBI MPC meeting schedule                                                          | INR         | 10:00 Mumbai, convention                        |
| Bank of Korea MPB meeting dates                                                   | KRW         | 10:00 Seoul, convention                         |
| US CPI and Employment Situation release dates, from the BLS schedule through FRED | all         | 08:30 New York, published                       |

The FRED sources need a free API key in `FRED_API_KEY` (<https://fred.stlouisfed.org/docs/api/api_key.html>): the BLS site itself refuses automated clients. Without the key they are skipped and the US events other than FOMC are missing from every calendar.

Not covered automatically (see `unsupported` in `config.json`): RBNZ, Banxico, Banco Central do Brasil, Banco Central de Chile, MAS, the Central Bank of the Republic of China (Taiwan) and the People's Bank of China. Their events can be added by hand with `add_calendar_events`, citing the official page.
