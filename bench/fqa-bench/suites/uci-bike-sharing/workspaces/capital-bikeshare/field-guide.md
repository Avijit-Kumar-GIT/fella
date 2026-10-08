# Capital Bikeshare data guide

This workspace covers daily rentals in Washington, D.C., from January 2011 through December 2012. It contains the files listed here and no station-level, fare, revenue, or individual-hour observations.

## Files and grain

- `daily.csv` has one record per calendar day (731 rows). `dteday` is the date; `cnt` is total rentals; `casual` and `registered` are the two rider counts. In the source, `cnt` is the sum of `casual` and `registered`.
- `monthly-usage-from-hourly.csv` has one row per year and month (24 rows). It is derived from UCI's original hourly file by summing `cnt`, `casual`, and `registered` for each `yr` + `mnth` group. `hourly_records` records how many source-hour rows went into each group. The original hourly records are not mounted here, so this file cannot answer questions at hour-of-day resolution.

## Shared fields

- `yr`: `0` means 2011; `1` means 2012.
- `mnth`: calendar month, 1–12.
- `cnt` / `rides`: total rental count at the file's grain.
- `casual` / `casual_rides`: casual-user rental count.
- `registered` / `registered_rides`: registered-user rental count.
- `workingday` in `daily.csv`: 1 when the date is neither a weekend nor a holiday; otherwise 0. “Non-working” therefore includes both weekends and holidays.

The source also contains weather and temperature fields. Temperature, feeling-temperature, humidity, and wind speed are normalized values, not raw physical units. This benchmark's questions avoid treating those normalized fields as degrees or percentages.
