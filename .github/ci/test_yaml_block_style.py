# ruff: file-ignore[no-self-use] -- unittest requires methods
"""The YAML block-style check rejects wrapped collections, not braces in strings or comments."""

import unittest

from yaml_block_style import wrapped_collections


class WrappedCollectionsTest(unittest.TestCase):
    """Only flow collections that the scanner sees count."""

    def test_only_real_collections_over_several_lines(self) -> None:
        """One-line collections, comments and block strings pass; wrapped collections fail."""
        assert wrapped_collections("a: {b: 1, c: [2, 3]}\n") == []
        not_collections = "# example: {\nmessage: |\n  example: {\n  text: [\n"
        assert wrapped_collections(not_collections) == []
        wrapped = "a:\n  - {\n      b: 1,\n    }\nc: [\n  1,\n]\n"
        assert wrapped_collections(wrapped) == [2, 5]


if __name__ == "__main__":
    unittest.main()
