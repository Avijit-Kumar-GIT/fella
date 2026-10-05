# Fella chart visual test folder

Mount the folder containing this file as a repository in Fella. Use a fresh
conversation for each case so each chart can be inspected independently. Ask
the prompt as written; the notes below are visual/data checks, not a change to
the automated benchmark golds.

## Cases

1. **Bar — category comparison**
   `From expenses.csv, make a bar chart of total spending by category and tell
   me which category is largest.`
   Check: Rent, Groceries, Dining, and Transport appear once each; Rent is the
   largest at 3,600.

2. **Pie — part-to-whole**
   `Show total spending by category in expenses.csv as a pie chart. Identify
   the largest slice.`
   Check: four slices; Rent is largest. Values are 3,600, 350, 180, and 60.

3. **Donut — part-to-whole with named total**
   `Show total spending by category in expenses.csv as a donut chart. Use the
   full spending total as the whole and identify the largest category.`
   Check: the same four category values; whole = 4,190; Rent is largest.

4. **Line — two time series**
   `In forecast.csv, plot planned and actual values by month as two lines. Say
   where actual is above plan.`
   Check: both series share the four month labels; actual is above plan in
   February and April.

5. **Scatter — relationship and grouping**
   `Using scatter.csv, plot study hours against exam score. Distinguish cohorts
   and label each student. Say whether scores generally rise with study time.`
   Check: four points, correct x/y axes, cohort distinction, and student labels;
   scores rise from 55 to 74 as hours rise from 1 to 4.

6. **Histogram — distribution**
   `Make a histogram of response times in distribution.csv using two equal-
   width bins, and report each bin's count.`
   Check: six observations are represented; counts are 4 and 2. Confirm the
   interval endpoints and whether a boundary value belongs to the lower or
   upper bin are explained consistently.

7. **Box plot — spread and outlier**
   `Make a box plot of score by cohort from box-readings.csv. Compare the
   spread and call out any outlier.`
   Check: separate A and B distributions; A's value 20 is visibly isolated
   from its other readings. If exact quartiles are shown, note the convention
   Fella uses rather than assuming every tool uses the same quartile method.

8. **Area — time-series magnitude**
   `Show revenue over time from area.csv as an area chart. Keep the month order
   Jan, Feb, Mar and briefly describe the change.`
   Check: only revenue is plotted (not a helper/index column); values are 20,
   30, and 25 in chronological order.

9. **Stacked area — changing composition**
   `Use composition.csv to make a stacked area chart of email, search, and
   social by month.`
   Check: three series, months Jan–Mar in order, and each layer contributes to
   the stacked total.

10. **Heatmap — two categorical dimensions**
    `Make a heatmap from heatmap.csv with weekday on one axis, daypart on the
    other, and count as intensity. Keep unobserved combinations visibly
    missing, not zero.`
    Check: the three observed cells are 10, 20, and 30; absent combinations
    remain distinguishable from measured zero.

11. **Forecast — observed, estimate, and range**
    `Chart forecast-band.csv by period. Show observed values separately from
    the forecast and include the lower-to-upper uncertainty range.`
    Check: observed Jan–Mar values are 10, 12, 13; forecast Apr–May values are
    15 and 16; the band is 13–17 and 13.5–18.5. Do not connect missing values
    as if they were observations.

12. **Missing reading — visible gap**
    `Plot the raw readings in readings.csv over time and keep the missing
    reading visible. Do not invent or fill a value.`
    Check: Jan 2 is represented as a gap/missing observation, not zero or an
    interpolated point. This is the requested behavior for this manual visual
    check; the automated benchmark's conflicting no-chart gold remains an
    unresolved failure and has not been changed.

13. **Python-derived scenario — generated chart data**
    `Using scenario.csv, calculate a scenario that is 10% above each monthly
    revenue value and chart observed revenue against the scenario.`
    Check: scenario values are 110, 132, and 143; they are clearly labeled as
    a scenario, not observed data.

14. **No chart when it adds little**
    `What was my total rent spending in March 2024 from expenses.csv? Answer
    with the amount and a short sentence; a chart would be unnecessary.`
    Check: answer in prose with 1,200 and no chart.

## Visual review checklist

- Test in both light and dark mode, and at a narrow window width as well as a
  normal desktop width.
- Confirm titles, axes, units, category/date labels, legend, missing-value
  treatment, and exact values agree with the source.
- Open any exact-value/table view and compare it with the chart marks.
- Check that charts remain readable without clipping, that color distinctions
  survive both themes, and that the prose takeaway does not claim more than the
  plotted data supports.
- Treat an incorrect chart or answer as a failure. Do not edit benchmark
  expectations to make it pass; record the prompt, output, and screenshot for
  review.
