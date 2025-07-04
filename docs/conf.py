# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see the documentation:
# https://www.sphinx-doc.org/en/master/usage/configuration.html

# -- Project information -----------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#project-information

project = "shinqlx"
copyright = "2024, Markus 'ShiN0' Gärtner"
author = "Markus 'ShiN0' Gärtner"

# -- General configuration ---------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#general-configuration

extensions = [
    "sphinx.ext.coverage",
    "sphinx.ext.intersphinx",
]

templates_path = ["_templates"]
exclude_patterns = ["_build", "Thumbs.db", ".DS_Store"]

intersphinx_mapping = {
    "python": ("https://docs.python.org/", None),
    "redis": ("https://redis.readthedocs.io/en/stable/", None),
}

# -- Options for HTML output -------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#options-for-html-output

html_theme = "alabaster"
html_theme_options = {
    "fixed_sidebar": True,
}
html_sidebars = {
    "**": [
        "about.html",
        "localtoc.html",
    ]
}
html_static_path = ["_static"]


highlight_language = "Python"

maximum_signature_line_length = 48

nitpicky = True
