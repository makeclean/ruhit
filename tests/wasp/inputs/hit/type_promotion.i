[Problem]
    # Specify coordinate system type
    type = ted
    [./s1] type = fred [../]
    [./s2] type = x  y=1 [../]
[]
[p1]
    type = t1
[]
[p2]
   type = t2
    [./s3]
        type = us3
    [../]
[]
