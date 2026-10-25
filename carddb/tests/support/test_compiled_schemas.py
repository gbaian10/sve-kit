from .compiled_schemas import compile_build


def test_default_and_explicit_t0_requests_share_compiled_schema() -> None:
    assert compile_build() is compile_build(("t0",))
