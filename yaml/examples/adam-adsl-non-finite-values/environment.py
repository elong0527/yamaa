def non_finite_value(kind):
    values = {
        'finite': 1.5,
        'zero': 0.0,
        'positive-infinity': float('inf'),
        'negative-infinity': float('-inf'),
        'nan': float('nan'),
    }
    return values[kind]
