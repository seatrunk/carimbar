"""carimbar: emissão de NF-e e NFS-e Nacional.

API síncrona. Dinheiro é sempre texto/Decimal; nenhum valor fiscal passa por
``float``. Operações fiscais usam o journal e o cofre locais.
"""

from ._carimbar import (
    Cofre,
    Dinheiro,
    Emissor,
    ErroCarimbar,
    ErroCofre,
    ErroConflito,
    ErroFiscal,
    ErroProducao,
    ErroTransporte,
    ErroTransporteCarimbar,
    ErroValidacao,
    __version__,
    validar_chave_acesso,
    validar_cnpj,
)

__all__ = [
    "Cofre",
    "Dinheiro",
    "Emissor",
    "ErroCarimbar",
    "ErroCofre",
    "ErroConflito",
    "ErroFiscal",
    "ErroProducao",
    "ErroTransporte",
    "ErroTransporteCarimbar",
    "ErroValidacao",
    "__version__",
    "validar_chave_acesso",
    "validar_cnpj",
]
