
[DollarSingleValueParams]
  param01 = $misc-\w+
  param02 = misc$-\w+
  param03 = $mis-\w+$
  param04 = misc-\w+$

  param05 = {misc-\w+
  param06 = misc{-\w+
  param07 = m`i$.{c}*
  param08 = {isc-\w+{

  param09 = }misc-\w+
  param10 = misc}-\w+
  param11 = $}sc-\w+}
  param12 = misc-\w+}

  param13 = trailing-single-space$ 
  param14 = trailing-comment-char$# comment here
[]
[DollarSingleValueArrays]
  array01 = '$misc-\w+'
  array02 = 'misc$-\w+'
  array03 = '$ms-\w+$ '
  array04 = 'misc-\w+$'

  array05 = '{misc-\w+'
  array06 = 'misc{-\w+'
  array07 = 'm`i$.{c}*'
  array08 = '{isc-\w+{'

  array09 = '}misc-\w+'
  array10 = 'misc}-\w+'
  array11 = '$}sc-\w+}'
  array12 = 'misc-\w+}'

  array13 = 'trailing-single-space$ '
  array14 = 'trailing-comment-char$# comment here'
[]
[DollarMultiValuesArrays]
  array01 = '$misc-\w+ misc$-\w+ $mis-\w+$ misc-\w+$'
  array02 = '{misc-\w+ misc{-\w+ misc-{+ $ {isc-\w+}'
  array03 = '}misc-\w+ misc}-\w+ $}sc-\w+} misc-\w+}'
  command = 'for bin in `ls ../../../dir/app_exe-*`
             do
             cat dir$
             echo
             $env
             cd ..
             ln -sf $bin .
             done'
[]
