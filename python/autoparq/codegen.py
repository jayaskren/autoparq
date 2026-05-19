def generate_python_snippet(report: dict) -> str:
    import autoparq._lib as _lib
    import json
    return _lib.py_generate_snippet(json.dumps(report), "pyarrow")


def generate_spark_snippet(report: dict) -> str:
    import autoparq._lib as _lib
    import json
    return _lib.py_generate_snippet(json.dumps(report), "spark")


def generate_pyspark_snippet(report: dict) -> str:
    import autoparq._lib as _lib
    import json
    return _lib.py_generate_snippet(json.dumps(report), "pyspark")


def generate_polars_snippet(report: dict) -> str:
    import autoparq._lib as _lib
    import json
    return _lib.py_generate_snippet(json.dumps(report), "polars")
