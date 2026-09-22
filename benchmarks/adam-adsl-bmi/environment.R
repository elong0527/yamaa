bmi <- function(weight_kg, height_cm, cm_per_m = 100) {
  weight_kg / (height_cm / cm_per_m)^2
}
