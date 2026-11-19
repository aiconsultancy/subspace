// Open-Meteo returns local wall-clock strings ("2026-10-31T14:00") when asked
// for timezone=auto. Comparing those strings is daylight-saving safe, so the
// only conversion needed is "now" into the location's zone.

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** True when the runtime knows the IANA zone name. */
export function isValidTimeZone(timeZone) {
  if (typeof timeZone !== "string" || timeZone === "") return false;
  try {
    new Intl.DateTimeFormat("en-GB", { timeZone });
    return true;
  } catch {
    return false;
  }
}

/**
 * Wall-clock time in a zone as "YYYY-MM-DDTHH:mm".
 * @param {number} epochMs
 * @param {string} timeZone IANA zone name
 */
export function zonedNow(epochMs, timeZone) {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone,
    hourCycle: "h23",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).formatToParts(new Date(epochMs));
  const get = (type) => parts.find((part) => part.type === type)?.value ?? "00";
  return `${get("year")}-${get("month")}-${get("day")}T${get("hour")}:${get("minute")}`;
}

export const dateOf = (wallTime) => wallTime.slice(0, 10);
export const hourOf = (wallTime) => wallTime.slice(0, 13);
export const clockOf = (wallTime) => wallTime.slice(11, 16);

/** Weekday name for a "YYYY-MM-DD" calendar date. */
export function weekdayName(date) {
  const [year, month, day] = date.split("-").map(Number);
  return WEEKDAYS[new Date(Date.UTC(year, month - 1, day)).getUTCDay()];
}

/** "Today" for the current date, else a short weekday. */
export function dayLabel(date, today) {
  return date === today ? "Today" : weekdayName(date);
}

/** Calendar date like "Sat 31 Oct". */
export function longDate(date) {
  const [year, month, day] = date.split("-").map(Number);
  const names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  return `${weekdayName(date)} ${day} ${names[month - 1]}`;
}

/** Add whole hours to a "YYYY-MM-DDTHH:mm" wall time using plain calendar maths. */
export function addHours(wallClock, hours) {
  const [date, clock] = wallClock.split("T");
  const [year, month, day] = date.split("-").map(Number);
  const [hour, minute] = clock.split(":").map(Number);
  const moved = new Date(Date.UTC(year, month - 1, day, hour + hours, minute));
  return moved.toISOString().slice(0, 16);
}

/** Add whole days to a "YYYY-MM-DD" date. */
export function addDays(date, days) {
  return dateOf(addHours(`${date}T00:00`, days * 24));
}

/** Human-readable age such as "just now", "12 min ago", "3 h ago", "2 days ago". */
export function describeAge(ageMs) {
  const minutes = Math.floor(Math.max(0, ageMs) / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours} h ago`;
  return `${Math.floor(hours / 24)} days ago`;
}
