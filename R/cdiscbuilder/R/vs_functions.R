#' Calculate Body Mass Index (BMI)
#'
#' @description Calculates Body Mass
#' Index (BMI) given height (in cm) and weight (in kg).
#'
#' @param height A numeric vector of heights in centimeters
#' @param weight A numeric vector of weights in kilograms
#'
#' @return A numeric vector representing the calculated BMI (kg/m^2)
#' @export
get_bmi <- function(height, weight) {
  height_m <- height / 100
  bmi <- weight / (height_m^2)
  bmi
}
