# Multi-file SWE-bench Verified tasks

71 tasks with 2+ files in the ground truth patch (out of 500 total).
Sorted by file count descending. Use `./scripts/add_task.sh <id>` to add one.

ID                                            Repo                           Files  Issue
----------------------------------------------------------------------------------------------------------------------------------------------------------------
sympy__sympy-13091                            sympy/sympy                    21     Return NotImplemented, not False, upon rich comparison with unknown type
sympy__sympy-16597                            sympy/sympy                    6      a.is_even does not imply a.is_finite
django__django-11532                          django/django                  5      Email messages crash on non-ASCII domain when email encoding is non-unicode.
astropy__astropy-13398                        astropy/astropy                4      A direct approach to ITRS to Observed transformations that stays within the ITRS
django__django-11138                          django/django                  4      TIME_ZONE value in DATABASES settings is not used when making dates timezone-awa
django__django-13121                          django/django                  4      durations-only expressions doesn't work on SQLite and MySQL
django__django-15629                          django/django                  4      Errors with db_collation – no propagation to foreignkeys
django__django-16263                          django/django                  4      Strip unused annotations from count queries
pylint-dev__pylint-4551                       pylint-dev/pylint              4      Use Python type hints for UML generation
pylint-dev__pylint-6386                       pylint-dev/pylint              4      Argument expected for short verbose option
django__django-11400                          django/django                  3      Ordering problem in admin.RelatedFieldListFilter and admin.RelatedOnlyFieldListF
django__django-11734                          django/django                  3      OuterRef in exclude() or ~Q() uses wrong model.
django__django-13195                          django/django                  3      HttpResponse.delete_cookie() should preserve cookie's samesite.
django__django-13344                          django/django                  3      Coroutine passed to the first middleware's process_response() instead of HttpRes
matplotlib__matplotlib-14623                  matplotlib/matplotlib          3      Inverting an axis using its limits does not work for log scale
matplotlib__matplotlib-25775                  matplotlib/matplotlib          3      [ENH]: Add get/set_antialiased to Text objects
pylint-dev__pylint-8898                       pylint-dev/pylint              3      bad-names-rgxs mangles regular expressions with commas
sphinx-doc__sphinx-10673                      sphinx-doc/sphinx              3      toctree contains reference to nonexisting document 'genindex', 'modindex', 'sear
sphinx-doc__sphinx-7590                       sphinx-doc/sphinx              3      C++ User Defined Literals not supported
sphinx-doc__sphinx-9461                       sphinx-doc/sphinx              3      Methods decorated with @classmethod and @property do not get documented.
sympy__sympy-14248                            sympy/sympy                    3      The difference of MatrixSymbols prints as a sum with (-1) coefficient
sympy__sympy-20438                            sympy/sympy                    3      `is_subset` gives wrong results
astropy__astropy-14369                        astropy/astropy                2      Incorrect units read from MRT (CDS format) files with astropy.table
astropy__astropy-8707                         astropy/astropy                2      Header.fromstring does not accept Python 3 bytes
django__django-10554                          django/django                  2      Union queryset with ordering breaks on ordering with derived querysets
django__django-11333                          django/django                  2      Optimization: Multiple URLResolvers may be unintentionally be constructed by cal
django__django-11885                          django/django                  2      Combine fast delete queries
django__django-12155                          django/django                  2      docutils reports an error rendering view docstring when the first line is not em
django__django-12325                          django/django                  2      pk setup for MTI to parent get confused by multiple OneToOne references.
django__django-12406                          django/django                  2      ModelForm RadioSelect widget for foreign keys should not present a blank option 
django__django-12741                          django/django                  2      Simplify signature of `DatabaseOperations.execute_sql_flush()`
django__django-13212                          django/django                  2      Make validators include the provided value in ValidationError
django__django-13512                          django/django                  2      Admin doesn't display properly unicode chars in JSONFields.
django__django-14011                          django/django                  2      LiveServerTestCase's ThreadedWSGIServer doesn't close database connections after
django__django-14170                          django/django                  2      Query optimization in YearLookup breaks filtering by "__iso_year"
django__django-14315                          django/django                  2      database client runshell doesn't respect os.environ values in some cases
django__django-14376                          django/django                  2      MySQL backend uses deprecated "db" and "passwd" kwargs.
django__django-14631                          django/django                  2      BaseForm's _clean_fields() and changed_data should access values via BoundField
django__django-15103                          django/django                  2      Make the element_id argument of json_script optional
django__django-15561                          django/django                  2      AlterField operation should be noop when adding/changing choices on SQLite.
django__django-15563                          django/django                  2      Wrong behavior on queryset update when multiple inheritance
django__django-16032                          django/django                  2      __in doesn't clear selected fields on the RHS when QuerySet.alias() is used afte
django__django-16256                          django/django                  2      acreate(), aget_or_create(), and aupdate_or_create() doesn't work as intended on
django__django-16315                          django/django                  2      QuerySet.bulk_create() crashes on mixed case columns in unique_fields/update_fie
django__django-16560                          django/django                  2      Allow to customize the code attribute of ValidationError raised by BaseConstrain
django__django-16631                          django/django                  2      SECRET_KEY_FALLBACKS is not used for sessions
django__django-16938                          django/django                  2      Serialization of m2m relation fails with custom manager using select_related
matplotlib__matplotlib-24870                  matplotlib/matplotlib          2      [ENH]: Auto-detect bool arrays passed to contour()?
matplotlib__matplotlib-25479                  matplotlib/matplotlib          2      Confusing (broken?) colormap name handling
mwaskom__seaborn-3187                         mwaskom/seaborn                2      Wrong legend values of large ranges
pydata__xarray-3095                           pydata/xarray                  2      REGRESSION: copy(deep=True) casts unicode indices to object
pydata__xarray-3305                           pydata/xarray                  2      DataArray.quantile does not honor `keep_attrs`
pydata__xarray-3993                           pydata/xarray                  2      DataArray.integrate has a 'dim' arg, but Dataset.integrate has a 'coord' arg
pydata__xarray-6938                           pydata/xarray                  2      `.swap_dims()` can modify original object
pydata__xarray-6992                           pydata/xarray                  2      index refactor: more `_coord_names` than `_variables` on Dataset
pylint-dev__pylint-4604                       pylint-dev/pylint              2      unused-import false positive for a module used in a type comment
pylint-dev__pylint-4661                       pylint-dev/pylint              2      Make pylint XDG Base Directory Specification compliant
pylint-dev__pylint-6528                       pylint-dev/pylint              2      Pylint does not respect ignores in `--recursive=y` mode
pytest-dev__pytest-5840                       pytest-dev/pytest              2      5.1.2 ImportError while loading conftest (windows import folder casing issues)
pytest-dev__pytest-8399                       pytest-dev/pytest              2      Starting v6.2.0, unittest setUpClass fixtures are no longer "private"
scikit-learn__scikit-learn-12682              scikit-learn/scikit-learn      2      `SparseCoder` doesn't expose `max_iter` for `Lasso`
scikit-learn__scikit-learn-25102              scikit-learn/scikit-learn      2      Preserving dtypes for DataFrame output by transformers that do not modify the in
sphinx-doc__sphinx-7462                       sphinx-doc/sphinx              2      `IndexError: pop from empty list` for empty tuple type annotation
sphinx-doc__sphinx-8120                       sphinx-doc/sphinx              2      locale/<language>/LC_MESSAGES/sphinx.po translation ignored
sphinx-doc__sphinx-8548                       sphinx-doc/sphinx              2      autodoc inherited-members won't work for inherited attributes (data members).
sphinx-doc__sphinx-8551                       sphinx-doc/sphinx              2      :type: and :rtype: gives false ambiguous class lookup warnings
sphinx-doc__sphinx-8593                       sphinx-doc/sphinx              2      autodoc: `:meta public:` does not effect to variables
sympy__sympy-13877                            sympy/sympy                    2      Matrix determinant raises Invalid NaN comparison with particular symbolic entrie
sympy__sympy-17318                            sympy/sympy                    2      sqrtdenest raises IndexError
sympy__sympy-19783                            sympy/sympy                    2      Dagger() * IdentityOperator() is not simplified
sympy__sympy-22080                            sympy/sympy                    2      Mod function lambdify bug
