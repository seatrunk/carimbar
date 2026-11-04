"""carimbar: emissão de NF-e e NFS-e Nacional. Em desenvolvimento (etapa 1)."""

from ._carimbar import __version__, validar_chave_acesso, validar_cnpj

__all__ = ["__version__", "validar_chave_acesso", "validar_cnpj"]
