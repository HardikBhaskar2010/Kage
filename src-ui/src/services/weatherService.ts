/**
 * Open-Meteo Weather Service for KAGE
 * Keyless, Free, CORS-enabled global weather API
 * Spec: https://open-meteo.com/en/docs
 */

export interface WeatherData {
  temp: number;
  city: string;
  weatherCode: number;
  isDay: boolean;
  condition: string;
}

const DEFAULT_WEATHER: WeatherData = {
  temp: 28,
  city: "New Delhi",
  weatherCode: 1,
  isDay: true,
  condition: "Mainly Clear",
};

/**
 * Maps WMO weather code to human-readable condition description
 */
export function getWmoCondition(code: number): string {
  if (code === 0) return "Clear Sky";
  if (code <= 3) return "Partly Cloudy";
  if (code <= 48) return "Foggy";
  if (code <= 55) return "Drizzle";
  if (code <= 65) return "Rain";
  if (code <= 75) return "Snow";
  if (code <= 82) return "Showers";
  if (code >= 95) return "Thunderstorm";
  return "Clear";
}

// 15-minute cache in localStorage to stay polite to Open-Meteo
const CACHE_KEY = "kage_open_meteo_cache_v3";
const CACHE_TTL_MS = 15 * 60 * 1000;

export async function fetchOpenMeteoWeather(): Promise<WeatherData> {
  // Check cached data first
  try {
    const cached = localStorage.getItem(CACHE_KEY);
    if (cached) {
      const parsed = JSON.parse(cached);
      if (Date.now() - parsed.timestamp < CACHE_TTL_MS && parsed.data) {
        return parsed.data;
      }
    }
  } catch {}

  // Determine latitude & longitude (default to New Delhi 28.6139, 77.2090 from mockup)
  let lat = 28.6139;
  let lon = 77.2090;
  let detectedCity = "New Delhi";

  try {
    const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
    // India Standard Timezone resolves to New Delhi
    if (tz === "Asia/Calcutta" || tz === "Asia/Kolkata" || tz === "Asia/Delhi") {
      lat = 28.6139;
      lon = 77.2090;
      detectedCity = "New Delhi";
    } else if (tz) {
      const parts = tz.split("/");
      if (parts.length > 1) {
        const rawCity = parts[1].replace(/_/g, " ");
        if (rawCity) {
          detectedCity = rawCity;
          // Resolve coordinates for detected city via Open-Meteo Geocoding
          try {
            const geoRes = await fetch(
              `https://geocoding-api.open-meteo.com/v1/search?name=${encodeURIComponent(rawCity)}&count=1`
            );
            if (geoRes.ok) {
              const geoData = await geoRes.json();
              if (geoData.results && geoData.results.length > 0) {
                lat = geoData.results[0].latitude;
                lon = geoData.results[0].longitude;
                detectedCity = geoData.results[0].name || rawCity;
              }
            }
          } catch {}
        }
      }
    }
  } catch {}

  // Fetch live weather from Open-Meteo
  try {
    const url = `https://api.open-meteo.com/v1/forecast?latitude=${lat}&longitude=${lon}&current=temperature_2m,weather_code,is_day&temperature_unit=celsius`;
    const res = await fetch(url);
    if (!res.ok) throw new Error(`Open-Meteo returned status ${res.status}`);

    const json = await res.json();
    const current = json.current;

    const data: WeatherData = {
      temp: Math.round(current.temperature_2m),
      city: detectedCity,
      weatherCode: current.weather_code,
      isDay: Boolean(current.is_day),
      condition: getWmoCondition(current.weather_code),
    };

    try {
      localStorage.setItem(
        CACHE_KEY,
        JSON.stringify({ timestamp: Date.now(), data })
      );
    } catch {}

    return data;
  } catch (err) {
    console.warn("KAGE: Failed to fetch Open-Meteo weather, using fallback baseline", err);
    return DEFAULT_WEATHER;
  }
}
