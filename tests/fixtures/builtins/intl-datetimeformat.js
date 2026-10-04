// Intl.DateTimeFormat (T77, stage A): en-US, the Gregorian calendar, every IANA time zone.
const d = new Date(Date.UTC(2026, 9, 4, 7, 12, 44, 123));
const jan = Date.UTC(2026, 0, 15);

console.log(Object.prototype.toString.call(Intl), Object.getOwnPropertyNames(Intl).join());
console.log(new Intl.DateTimeFormat().format(d), Intl.DateTimeFormat().format(d));
console.log(JSON.stringify(Intl.DateTimeFormat().resolvedOptions()));

// zigbee-herdsman's time cluster: @date-fns/tz keeps `format` unbound.
for (const timeZone of ["UTC", "Europe/Sofia", "America/New_York", "Asia/Kolkata", "Australia/Lord_Howe", "Etc/GMT+5", "Pacific/Chatham"]) {
  const format = new Intl.DateTimeFormat("en-US", { timeZone, timeZoneName: "longOffset" }).format;
  console.log(timeZone, format(d).split("GMT")[1], format(jan).split("GMT")[1]);
}

const show = (locales, options, ...dates) => {
  try {
    const f = new Intl.DateTimeFormat(locales, options);
    console.log(JSON.stringify(f.resolvedOptions()));
    for (const x of dates.length ? dates : [d]) console.log("  " + f.format(x));
  } catch (e) {
    console.log(`${e.constructor.name}: ${e.message}`);
  }
};

show("en-US", { dateStyle: "full", timeStyle: "full", timeZone: "America/Los_Angeles" });
show("en", { dateStyle: "long", timeStyle: "short" });
show("en", { dateStyle: "medium", timeStyle: "medium", hourCycle: "h23", timeZone: "Asia/Tokyo" });
show("en-US", { dateStyle: "short" }, d, new Date(-62198755200000));
show("en-US-u-hc-h23", { hour: "numeric", minute: "2-digit" });
show("en", { weekday: "long", year: "numeric", month: "long", day: "numeric", hour: "numeric", minute: "numeric",
  second: "numeric", fractionalSecondDigits: 3, timeZoneName: "short", timeZone: "Europe/London" }, d, jan);
show("en", { era: "short", year: "numeric", month: "short", day: "2-digit" }, new Date("-000044-03-15T12:00:00Z"));
show("en", { hour: "numeric", dayPeriod: "long" }, d, Date.UTC(2026, 0, 1, 12), Date.UTC(2026, 0, 1, 0), Date.UTC(2026, 0, 1, 19));
show("en", { month: "2-digit", day: "2-digit", hour12: false, hour: "2-digit", minute: "2-digit" });
show("en", { timeZoneName: "shortGeneric", timeZone: "Europe/Sofia" }, d, Date.UTC(1850, 0, 1), Date.UTC(10000, 6, 1));
show("en", { timeZone: "+05:30", timeZoneName: "long", hour: "numeric" });
show("en", { second: "numeric", fractionalSecondDigits: 1, dayPeriod: "narrow" });
show("xx", {}, 0);
show("en", { weekday: "longer" });
show("en", { timeZone: "Mars/Olympus" });
show("en", { dateStyle: "full", hour: "numeric" });
show("en", null);

const f = new Intl.DateTimeFormat("en", { timeZone: "Asia/Calcutta", hour: "numeric", minute: "numeric" });
console.log(JSON.stringify(f.formatToParts(d)));
console.log(f.format === f.format, f.format.name === "", f.format.length);
try {
  f.format(NaN);
} catch (e) {
  console.log(`${e.constructor.name}: ${e.message}`);
}
try {
  Intl.DateTimeFormat.prototype.format;
} catch (e) {
  console.log(`${e.constructor.name}: ${e.message}`);
}
console.log(Intl.getCanonicalLocales(["EN-us", "zh-hant-tw", "en-u-ca-gregory-hc-h23", "de-1996-1901"]).join());
console.log(Intl.DateTimeFormat.supportedLocalesOf(["en-US", "en-ZZ", "xx", "de"]).join());
console.log(Intl.supportedValuesOf("timeZone").length, Intl.supportedValuesOf("calendar").join());

// Date.prototype.toLocale*String with locales and options (here because the program names Intl).
console.log(d.toLocaleString(), "|", d.toLocaleDateString(), "|", d.toLocaleTimeString());
console.log(d.toLocaleString("en-US", { timeZone: "America/Chicago" }), "|", d.toLocaleDateString("en", { dateStyle: "long" }));
console.log(d.toLocaleTimeString("en", { hour: "2-digit", minute: "2-digit", timeZone: "Asia/Tokyo" }), "|", d.toLocaleDateString("en", { month: "short" }));
console.log(new Date(NaN).toLocaleString("en"), Date.prototype.toLocaleString.name, Date.prototype.toLocaleString.length);
try {
  d.toLocaleDateString("en", { timeStyle: "short" });
} catch (e) {
  console.log(`${e.constructor.name}: ${e.message}`);
}
