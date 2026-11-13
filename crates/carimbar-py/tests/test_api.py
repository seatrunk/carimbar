"""Testes pytest sobre o wheel instalado, com backend replay local."""

import base64
import gzip
import json
import subprocess
from pathlib import Path

import pytest

import carimbar

CNPJ = "00000001000136"
SENHA = "senha-sintetica-123"
KEK = "07" * 32
CHAVE_NFSE = "355030" + CNPJ + "0" * 30
CHAVE_NFE = "35261100000001000136550010000000001123456780"
ID_DPS = "DPS355030810000000100013600001000000000000001"


def pfx(diretorio: Path) -> Path:
    chave = diretorio / "chave.pem"
    cert = diretorio / "cert.pem"
    pfx = diretorio / "a1.p12"
    subprocess.run(
        [
            "openssl", "req", "-x509", "-nodes", "-sha256", "-days", "3650",
            "-newkey", "rsa:2048", "-keyout", str(chave), "-out", str(cert),
            "-subj", "/CN=EMPRESA SINTETICA PY",
            "-addext", f"subjectAltName=otherName:2.16.76.1.3.3;UTF8:{CNPJ}",
        ],
        check=True,
    )
    subprocess.run(
        [
            "openssl", "pkcs12", "-export", "-in", str(cert), "-inkey", str(chave),
            "-out", str(pfx), "-passout", f"pass:{SENHA}",
        ],
        check=True,
    )
    return pfx


def criar_cofre(diretorio: Path) -> carimbar.Cofre:
    cofre = carimbar.Cofre(str(diretorio / "cofre.db"), KEK)
    cofre.importar(CNPJ, "a1", pfx(diretorio).read_bytes(), SENHA)
    return cofre


def dps_xml() -> str:
    return (
        f'<DPS xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01">'
        f'<infDPS Id="{ID_DPS}"><tpAmb>2</tpAmb><prest><CNPJ>{CNPJ}</CNPJ></prest></infDPS></DPS>'
    )


def nfse_xml() -> str:
    return (
        f'<NFSe xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01">'
        f'<infNFSe Id="NFS{CHAVE_NFSE}"><xLocEmi>Sao Paulo</xLocEmi>'
        f'<xLocPrestacao>Sao Paulo</xLocPrestacao><nNFSe>123</nNFSe><cStat>100</cStat>'
        f'<dhProc>2026-11-09T10:00:00-03:00</dhProc><nDFSe>1</nDFSe>'
        f'<emit><CNPJ>{CNPJ}</CNPJ><IM>12345</IM><xNome>Empresa Sintetica</xNome>'
        f'<enderNac><xLgr>Rua A</xLgr><nro>1</nro><xBairro>Centro</xBairro>'
        f'<cMun>3550308</cMun><UF>SP</UF><CEP>01001000</CEP></enderNac></emit>'
        f'<valores><vBC>10.00</vBC><pAliqAplic>2.00</pAliqAplic><vISSQN>0.20</vISSQN>'
        f'<vLiq>9.80</vLiq></valores><DPS versao="1.01"><infDPS Id="{ID_DPS}">'
        f'<tpAmb>2</tpAmb><serv><cServ><cTribNac>010101</cTribNac>'
        f'<xDescServ>Servico sintetico</xDescServ></cServ></serv><valores>'
        f'<vServPrest><vServ>10.00</vServ></vServPrest><trib><tribMun><tribISSQN>1</tribISSQN>'
        f'</tribMun></trib></valores></infDPS></DPS></infNFSe></NFSe>'
    )


def nfe_xml() -> str:
    return (
        f'<NFe xmlns="http://www.portalfiscal.inf.br/nfe"><infNFe versao="4.00" Id="NFe{CHAVE_NFE}">'
        f'<ide><cUF>35</cUF><natOp>Venda</natOp><mod>55</mod><serie>1</serie><nNF>1</nNF>'
        f'<dhEmi>2026-11-09T10:00:00-03:00</dhEmi><tpNF>1</tpNF><tpEmis>1</tpEmis><tpAmb>2</tpAmb></ide>'
        f'<emit><CNPJ>{CNPJ}</CNPJ><xNome>Empresa Sintetica</xNome><enderEmit><xLgr>Rua A</xLgr>'
        f'<nro>1</nro><xBairro>Centro</xBairro><cMun>3550308</cMun><xMun>Sao Paulo</xMun><UF>SP</UF>'
        f'<CEP>01001000</CEP></enderEmit><IE>123456789</IE></emit><det nItem="1"><prod><cProd>1</cProd>'
        f'<xProd>Produto</xProd><NCM>00000000</NCM><CFOP>5102</CFOP><uCom>UN</uCom><qCom>1.0000</qCom>'
        f'<vUnCom>10.0000000000</vUnCom><vProd>10.00</vProd></prod><imposto></imposto></det>'
        f'<total><ICMSTot><vBC>0.00</vBC><vICMS>0.00</vICMS><vProd>10.00</vProd><vFrete>0.00</vFrete>'
        f'<vSeg>0.00</vSeg><vDesc>0.00</vDesc><vIPI>0.00</vIPI><vPIS>0.00</vPIS><vCOFINS>0.00</vCOFINS>'
        f'<vNF>10.00</vNF></ICMSTot></total><transp><modFrete>9</modFrete></transp>'
        f'<pag><detPag><tPag>01</tPag><vPag>10.00</vPag></detPag></pag></infNFe></NFe>'
    )


def gzip_b64(texto: str) -> str:
    return base64.b64encode(gzip.compress(texto.encode())).decode()


def replay_nfse(diretorio: Path) -> Path:
    corpo = json.dumps(
        {
            "idDps": ID_DPS,
            "chaveAcesso": CHAVE_NFSE,
            "nfseXmlGZipB64": gzip_b64(nfse_xml()),
            "dataHoraProcessamento": "2026-11-09T10:00:00-03:00",
            "alertas": [],
        }
    )
    caminho = diretorio / "replay-nfse.json"
    caminho.write_text(json.dumps({"respostas": [{"url_contem": "/nfse", "status": 201, "corpo": corpo}]}))
    return caminho


def replay_nfe(diretorio: Path) -> Path:
    corpo = (
        f'<retEnviNFe xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><cStat>104</cStat>'
        f'<xMotivo>Lote processado</xMotivo><protNFe versao="4.00"><infProt><chNFe>{CHAVE_NFE}</chNFe>'
        f'<nProt>135</nProt><cStat>100</cStat><xMotivo>Autorizado</xMotivo></infProt></protNFe></retEnviNFe>'
    )
    caminho = diretorio / "replay-nfe.json"
    caminho.write_text(
        json.dumps({"respostas": [{"url_contem": "nfeautorizacao4", "status": 200, "corpo": corpo}]})
    )
    return caminho


def test_dinheiro_preserva_precisao():
    assert carimbar.Dinheiro("0.10").somar(carimbar.Dinheiro("0.20")).texto() == "0.30"
    with pytest.raises(TypeError):
        carimbar.Dinheiro(0.1)


def test_cofre_lista_metadados_sem_segredo(tmp_path):
    cofre = criar_cofre(tmp_path)
    lista = cofre.listar(CNPJ)
    assert len(lista) == 1
    assert lista[0]["id"] == "a1"
    assert "pfx" not in lista[0]
    cofre.verificar_auditoria()


def test_nfse_com_replay_gera_pdf(tmp_path):
    criar_cofre(tmp_path)
    emissor = carimbar.Emissor.com_replay(
        str(tmp_path / "cofre.db"),
        str(tmp_path / "journal.db"),
        CNPJ,
        KEK,
        str(replay_nfse(tmp_path)),
    )
    resultado = emissor.emitir(dps_xml())
    assert resultado["estado"] == "autorizada"
    assert resultado["chave"] == CHAVE_NFSE
    pdf = emissor.gerar_danfse(resultado["operacao"])
    assert pdf.startswith(b"%PDF")


def test_pedido_simples_com_replay_e_idempotencia(tmp_path):
    criar_cofre(tmp_path)
    emissor = carimbar.Emissor.com_replay(
        str(tmp_path / "cofre.db"),
        str(tmp_path / "journal.db"),
        CNPJ,
        KEK,
        str(replay_nfse(tmp_path)),
    )
    prestador = {"municipio": "3550308", "simples": 2}
    pedido = {
        "servico": "010101",
        "descricao": "Servico sintetico",
        "valor_centavos": 1000,
    }
    primeira = emissor.emitir_pedido(pedido, prestador, idempotencia="venda-1")
    assert primeira["estado"] == "autorizada"
    repetida = emissor.emitir_pedido(pedido, prestador, idempotencia="venda-1")
    assert repetida["estado"] == "autorizada"
    assert repetida["chave"] == primeira["chave"]


def test_pedido_recusa_float_em_valor_fiscal(tmp_path):
    criar_cofre(tmp_path)
    emissor = carimbar.Emissor.com_replay(
        str(tmp_path / "cofre.db"),
        str(tmp_path / "journal.db"),
        CNPJ,
        KEK,
        str(replay_nfse(tmp_path)),
    )
    with pytest.raises(carimbar.ErroValidacao):
        emissor.emitir_pedido(
            {"servico": "010101", "descricao": "Servico", "valor": 10.0},
            {"municipio": "3550308", "simples": 2},
        )


def test_nfe_com_replay_gera_danfe(tmp_path):
    criar_cofre(tmp_path)
    emissor = carimbar.Emissor.com_replay(
        str(tmp_path / "cofre.db"),
        str(tmp_path / "journal.db"),
        CNPJ,
        KEK,
        str(replay_nfe(tmp_path)),
    )
    resultado = emissor.emitir_nfe(nfe_xml())
    assert resultado["estado"] == "autorizada"
    assert resultado["chave"] == CHAVE_NFE
    pdf = emissor.gerar_danfe_nfe(resultado["operacao"])
    assert pdf.startswith(b"%PDF")
