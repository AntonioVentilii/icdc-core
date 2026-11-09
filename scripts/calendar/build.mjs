#!/usr/bin/env node
// Builds the margin event calendar from official central-bank and statistics schedules.
//
// Reads scripts/calendar/config.json, fetches every configured source, parses its future
// events and writes a reviewable JSON (default: calendar.json in the working directory).
// It never contacts the canister: loading is done by load.sh.
//
// No date is ever invented. Each parser checks what it found and fails loudly if the page
// no longer looks as expected; a failed source is reported and makes the run exit non-zero
// unless --allow-partial is given.
//
// Usage: node scripts/calendar/build.mjs [--out FILE] [--sources a,b,c] [--allow-partial]
//
// The FRED sources (US CPI and payrolls) need a free API key in FRED_API_KEY. Without it they
// are skipped with a warning and every other source still builds.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const USER_AGENT = 'icdc-calendar/1.0 (+https://github.com/AntonioVentilii/icdc-core)';
const MAX_EVENTS_PER_UNDERLYING = 512;
const MAX_LABEL_LEN = 64;
const WEIGHT_MAX = 10;

const MONTHS = {
	jan: 1,
	feb: 2,
	mar: 3,
	apr: 4,
	may: 5,
	jun: 6,
	jul: 7,
	aug: 8,
	sep: 9,
	oct: 10,
	nov: 11,
	dec: 12
};

const MONTH_NAMES =
	'January|February|March|April|May|June|July|August|September|October|November|December';

function monthNumber(name) {
	const month = MONTHS[name.trim().toLowerCase().slice(0, 3)];
	if (!month) {
		throw new Error(`unknown month name "${name}"`);
	}
	return month;
}

function decodeEntities(text) {
	return text
		.replace(/&nbsp;/g, ' ')
		.replace(/&ndash;|&#8211;/g, '-')
		.replace(/&mdash;|&#8212;/g, '-')
		.replace(/&amp;/g, '&')
		.replace(/&#65279;/g, '')
		.replace(/–/g, '-');
}

// Strips tags into one item per line, so parsers work on visible text.
function textLines(html) {
	return decodeEntities(html.replace(/<script[\s\S]*?<\/script>/gi, ' ').replace(/<[^>]*>/g, '\n'))
		.split('\n')
		.map((line) => line.replace(/\s+/g, ' ').trim())
		.filter((line) => line.length > 0);
}

function checkDate(year, month, day) {
	const date = new Date(Date.UTC(year, month - 1, day));
	if (
		date.getUTCFullYear() !== year ||
		date.getUTCMonth() !== month - 1 ||
		date.getUTCDate() !== day
	) {
		throw new Error(`invalid date ${year}-${month}-${day}`);
	}
}

// Offset of a time zone at a UTC instant, in milliseconds (local wall clock minus UTC).
function zoneOffsetMs(utcMs, timeZone) {
	const parts = Object.fromEntries(
		new Intl.DateTimeFormat('en-US', {
			timeZone,
			hourCycle: 'h23',
			year: 'numeric',
			month: '2-digit',
			day: '2-digit',
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit'
		})
			.formatToParts(new Date(utcMs))
			.map((p) => [p.type, p.value])
	);
	const wall = Date.UTC(
		Number(parts.year),
		Number(parts.month) - 1,
		Number(parts.day),
		Number(parts.hour),
		Number(parts.minute),
		Number(parts.second)
	);
	return wall - utcMs;
}

// UTC milliseconds of a local wall-clock time in a named time zone (official tz database).
function zonedToUtcMs(year, month, day, hhmm, timeZone) {
	checkDate(year, month, day);
	const [hour, minute] = hhmm.split(':').map(Number);
	const wall = Date.UTC(year, month - 1, day, hour, minute);
	let utc = wall - zoneOffsetMs(wall, timeZone);
	// A second pass settles instants near a daylight-saving transition.
	utc = wall - zoneOffsetMs(utc, timeZone);
	return utc;
}

async function fetchText(url) {
	const response = await fetch(url, {
		headers: { 'User-Agent': USER_AGENT },
		signal: AbortSignal.timeout(30_000)
	});
	if (!response.ok) {
		throw new Error(`HTTP ${response.status} from ${url}`);
	}
	return response.text();
}

function requireCount(source, events, min) {
	if (events.length < min) {
		throw new Error(
			`${source}: found ${events.length} dated entries, expected at least ${min}; the page layout may have changed`
		);
	}
	return events;
}

// ----- Parsers: each returns [{ year, month, day, time?, label }] -----

const parsers = {
	// "2026 FOMC Meetings" headings, then month and day-range cells. Decision on the last day.
	fomc(html) {
		const events = [];
		const sections = html.split(/<h4><a id="\d+">(\d{4}) FOMC Meetings/);
		for (let i = 1; i < sections.length; i += 2) {
			const year = Number(sections[i]);
			const body = sections[i + 1];
			const months = [...body.matchAll(/fomc-meeting__month[^>]*><strong>([^<]+)<\/strong>/g)];
			const dates = [...body.matchAll(/fomc-meeting__date[^>]*>([^<]+)<\/div>/g)];
			if (months.length !== dates.length) {
				throw new Error(`fomc ${year}: ${months.length} months but ${dates.length} dates`);
			}
			months.forEach((m, j) => {
				const raw = dates[j][1].trim();
				if (/unscheduled|notation|call/i.test(raw)) {
					return;
				}
				const days = raw.match(/\d{1,2}/g);
				if (!days) {
					throw new Error(`fomc ${year}: unreadable date "${raw}"`);
				}
				const monthName = m[1].split('/').pop();
				events.push({
					year,
					month: monthNumber(monthName),
					day: Number(days[days.length - 1]),
					label: `FOMC decision`
				});
			});
		}
		return requireCount('fomc', events, 4);
	},

	// <dt>DD/MM/YYYY</dt><dd>... monetary policy meeting ... (Day 2), followed by press conference</dd>
	ecb(html) {
		const events = [];
		for (const m of html.matchAll(
			/<dt>\s*(\d{2})\/(\d{2})\/(\d{4})\s*<\/dt>\s*<dd>([\s\S]*?)<\/dd>/g
		)) {
			const text = m[4];
			if (/monetary policy meeting/i.test(text) && /followed by press conference/i.test(text)) {
				events.push({
					year: Number(m[3]),
					month: Number(m[2]),
					day: Number(m[1]),
					label: 'ECB decision'
				});
			}
		}
		return requireCount('ecb', events, 1);
	},

	// "2026 confirmed dates" sections listing "Thursday 5 February".
	boe(html) {
		const lines = textLines(html);
		const events = [];
		let year = null;
		for (const line of lines) {
			const heading = line.match(/^(\d{4}) (confirmed|provisional) dates$/i);
			if (heading) {
				year = heading[2].toLowerCase() === 'confirmed' ? Number(heading[1]) : null;
				continue;
			}
			if (/^Monetary Policy Committee Reports/i.test(line)) {
				year = null;
			}
			const m = line.match(
				new RegExp(`^(Monday|Tuesday|Wednesday|Thursday|Friday) (\\d{1,2}) (${MONTH_NAMES})$`)
			);
			if (year && m) {
				events.push({ year, month: monthNumber(m[3]), day: Number(m[2]), label: 'BoE decision' });
			}
		}
		return requireCount('boe', events, 4);
	},

	// "Table : 2026" captions; the first cell of each row holds the meeting days.
	boj(html) {
		const events = [];
		for (const table of html.matchAll(
			/<caption[^>]*>\s*Table : (\d{4})\s*<\/caption>([\s\S]*?)<\/table>/g
		)) {
			const year = Number(table[1]);
			for (const row of table[2].matchAll(/<tr>\s*<td>([\s\S]*?)<\/td>/g)) {
				const cell = decodeEntities(row[1].replace(/<[^>]*>/g, ' '));
				let month = null;
				let last = null;
				for (const d of cell.matchAll(/(?:([A-Z][a-z]{2,4})\.?\s+)?(\d{1,2})\s*\(/g)) {
					if (d[1]) {
						month = monthNumber(d[1]);
					}
					if (month) {
						last = { month, day: Number(d[2]) };
					}
				}
				if (last) {
					events.push({ year, ...last, label: 'BoJ decision' });
				}
			}
		}
		return requireCount('boj', events, 4);
	},

	// "Board meeting schedules 2026", then two-day meetings "2-3 February" (decision on day 2).
	rba(html) {
		const lines = textLines(html);
		const events = [];
		let year = null;
		const range = new RegExp(`^(\\d{1,2})(?: (${MONTH_NAMES}))? ?- ?(\\d{1,2}) (${MONTH_NAMES})$`);
		for (const line of lines) {
			const heading = line.match(/^Board meeting schedules? (\d{4})$/i);
			if (heading) {
				year = Number(heading[1]);
				continue;
			}
			const m = line.match(range);
			if (year && m) {
				events.push({ year, month: monthNumber(m[4]), day: Number(m[3]), label: 'RBA decision' });
			}
		}
		return requireCount('rba', events, 4);
	},

	// Event schedule: "DD.MM.YYYY", "HH:MM", "Monetary policy assessment of ... (press release)".
	snb(html) {
		const lines = textLines(html);
		const events = [];
		for (let i = 2; i < lines.length; i += 1) {
			if (/^Monetary policy assessment of .*\(press release\)$/i.test(lines[i])) {
				const date = lines[i - 2].match(/^(\d{2})\.(\d{2})\.(\d{4})$/);
				const time = lines[i - 1].match(/^(\d{2}:\d{2})$/);
				if (!date || !time) {
					throw new Error(`snb: assessment without date and time near "${lines[i]}"`);
				}
				events.push({
					year: Number(date[3]),
					month: Number(date[2]),
					day: Number(date[1]),
					time: time[1],
					label: 'SNB decision'
				});
			}
		}
		return requireCount('snb', events, 1);
	},

	// Upcoming events link to ".../YYYY/MM/interest-rate-announcement-december-9-2026/".
	boc(html) {
		const seen = new Set();
		const events = [];
		for (const m of html.matchAll(
			/bankofcanada\.ca\/\d{4}\/\d{2}\/interest-rate-announcement-([a-z]+)-(\d{1,2})-(\d{4})/g
		)) {
			const key = m[0];
			if (!seen.has(key)) {
				seen.add(key);
				events.push({
					year: Number(m[3]),
					month: monthNumber(m[1]),
					day: Number(m[2]),
					label: 'BoC decision'
				});
			}
		}
		return requireCount('boc', events, 1);
	},

	// "Meeting Schedule of the Monetary Policy Committee for 2026-2027 April 6, 7 and 8, 2026 ..."
	rbi(html) {
		const text = textLines(html).join(' ');
		const events = [];
		for (const block of text.matchAll(
			/Meeting Schedule of the Monetary Policy Committee for \d{4}-\d{4}([\s\S]*?)(?=[A-Z][a-z]{2} \d{2}, \d{4} |Website last updated|$)/g
		)) {
			for (const m of block[1].matchAll(
				new RegExp(`(${MONTH_NAMES}) ([0-9 ,and]+?)\\s*,\\s*(\\d{4})`, 'g')
			)) {
				const days = m[2].match(/\d{1,2}/g);
				events.push({
					year: Number(m[3]),
					month: monthNumber(m[1]),
					day: Number(days[days.length - 1]),
					label: 'RBI decision'
				});
			}
		}
		return requireCount('rbi', events, 4);
	},

	// Year headings, then "Jan.15 (Thu)" meeting days.
	bok(html) {
		const lines = textLines(html);
		const events = [];
		let year = null;
		for (const line of lines) {
			if (/^\d{4}$/.test(line)) {
				year = Number(line);
				continue;
			}
			const m = line.match(/^([A-Z][a-z]{2})\.\s*(\d{1,2})\s*\((Mon|Tue|Wed|Thu|Fri)\)$/);
			if (year && m) {
				events.push({ year, month: monthNumber(m[1]), day: Number(m[2]), label: 'BoK decision' });
			}
		}
		return requireCount('bok', events, 4);
	}
};

// FRED release dates (St. Louis Fed), which republish the BLS release schedule.
async function fredReleaseDates(sourceId, source) {
	const key = process.env.FRED_API_KEY;
	if (!key) {
		throw new Error(
			`${sourceId}: FRED_API_KEY is not set (free key: https://fred.stlouisfed.org/docs/api/api_key.html)`
		);
	}
	const today = new Date().toISOString().slice(0, 10);
	const url = new URL(source.url);
	url.searchParams.set('api_key', key);
	url.searchParams.set('file_type', 'json');
	url.searchParams.set('include_release_dates_with_no_data', 'true');
	url.searchParams.set('realtime_start', today);
	url.searchParams.set('realtime_end', '9999-12-31');
	url.searchParams.set('sort_order', 'asc');
	url.searchParams.set('limit', '1000');
	const body = JSON.parse(await fetchText(url.toString()));
	if (!Array.isArray(body.release_dates)) {
		throw new Error(`${sourceId}: unexpected FRED response`);
	}
	const label = source.class === 'us_cpi' ? 'US CPI release' : 'US payrolls release';
	const events = body.release_dates.map((r) => {
		const [year, month, day] = r.date.split('-').map(Number);
		return { year, month, day, label };
	});
	return requireCount(sourceId, events, 1);
}

function parseArgs(argv) {
	const args = { out: 'calendar.json', sources: null, allowPartial: false };
	for (let i = 0; i < argv.length; i += 1) {
		if (argv[i] === '--out') {
			args.out = argv[(i += 1)];
		} else if (argv[i] === '--sources') {
			args.sources = argv[(i += 1)].split(',');
		} else if (argv[i] === '--allow-partial') {
			args.allowPartial = true;
		} else {
			throw new Error(`unknown argument ${argv[i]}`);
		}
	}
	return args;
}

function expandTargets(appliesTo, config) {
	return appliesTo.flatMap((t) => config.underlyings[t] ?? [t]);
}

async function main() {
	const args = parseArgs(process.argv.slice(2));
	const here = dirname(fileURLToPath(import.meta.url));
	const config = JSON.parse(readFileSync(join(here, 'config.json'), 'utf8'));
	const nowMs = Date.now();
	const sourceIds = args.sources ?? Object.keys(config.sources);

	const report = {};
	const calendars = {};
	let failed = false;

	for (const id of sourceIds) {
		const source = config.sources[id];
		if (!source) {
			throw new Error(`unknown source "${id}"`);
		}
		const weight = config.weights[source.class];
		if (!(weight >= 0 && weight <= WEIGHT_MAX)) {
			throw new Error(`weight for ${source.class} must be within [0, ${WEIGHT_MAX}]`);
		}
		if (source.requires && !process.env[source.requires]) {
			// A missing key skips the source instead of failing the run: every other source still
			// produces its calendar, and the report says exactly what was left out.
			report[id] = {
				status: 'skipped',
				url: source.url,
				reason: `${source.requires} is not set`
			};
			console.error(`WARNING: ${id} skipped because ${source.requires} is not set`);
			continue;
		}
		try {
			const raw =
				source.requires === 'FRED_API_KEY'
					? await fredReleaseDates(id, source)
					: parsers[id](await fetchText(source.url));
			const future = [];
			for (const e of raw) {
				const time = e.time ?? source.time;
				const utcMs = zonedToUtcMs(e.year, e.month, e.day, time, source.time_zone);
				if (utcMs <= nowMs) {
					continue;
				}
				const iso = new Date(utcMs).toISOString().slice(0, 10);
				const label = `${e.label} ${iso}`;
				if (label.length > MAX_LABEL_LEN) {
					throw new Error(`label too long: ${label}`);
				}
				future.push({
					time_ns: (BigInt(utcMs) * 1_000_000n).toString(),
					time_utc: new Date(utcMs).toISOString(),
					local_time: `${e.year}-${String(e.month).padStart(2, '0')}-${String(e.day).padStart(2, '0')} ${time} ${source.time_zone}`,
					weight,
					label,
					source: id,
					source_url: source.url,
					time_basis: source.time_basis
				});
			}
			const unique = [...new Map(future.map((e) => [e.time_ns + e.label, e])).values()];
			for (const underlying of expandTargets(source.applies_to, config)) {
				(calendars[underlying] ??= []).push(...unique);
			}
			report[id] = { status: 'ok', url: source.url, parsed: raw.length, future: unique.length };
		} catch (error) {
			failed = true;
			report[id] = { status: 'error', url: source.url, error: error.message };
		}
	}

	for (const [underlying, events] of Object.entries(calendars)) {
		events.sort((a, b) => (BigInt(a.time_ns) < BigInt(b.time_ns) ? -1 : 1));
		if (events.length > MAX_EVENTS_PER_UNDERLYING) {
			report[`cap:${underlying}`] = {
				status: 'truncated',
				kept: MAX_EVENTS_PER_UNDERLYING,
				dropped: events.length - MAX_EVENTS_PER_UNDERLYING
			};
			events.length = MAX_EVENTS_PER_UNDERLYING;
		}
	}

	const output = {
		generated_at: new Date(nowMs).toISOString(),
		weights: config.weights,
		sources: report,
		unsupported: config.unsupported,
		calendars
	};
	writeFileSync(args.out, `${JSON.stringify(output, null, '\t')}\n`);

	for (const [id, r] of Object.entries(report)) {
		const detail =
			r.status === 'ok' ? `${r.future} future of ${r.parsed}` : (r.error ?? r.reason ?? '');
		console.error(`${r.status.padEnd(9)} ${id.padEnd(12)} ${detail}`);
	}
	console.error(`wrote ${args.out}`);
	if (failed && !args.allowPartial) {
		console.error('some sources failed; rerun with --allow-partial to accept a partial calendar');
		process.exit(1);
	}
}

main().catch((error) => {
	console.error(error.message);
	process.exit(1);
});
