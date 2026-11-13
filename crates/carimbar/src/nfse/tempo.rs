//! Datas e horas do leiaute NFS-e.
//!
//! O leiaute aceita apenas os formatos `AAAA-MM-DD` (`TSData`) e
//! `AAAA-MM-DDThh:mm:ss±hh:mm` (`TSDateTimeUTC`), com ano entre 2000 e 2099 e
//! deslocamento de fuso em horas inteiras, até 12:00. Não há dependência de
//! crate de calendário: a validação é local e determinística.

use crate::nfse::erro::ErroNfse;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Data no formato `AAAA-MM-DD` usada em `dCompet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DataFiscal {
    ano: u16,
    mes: u8,
    dia: u8,
}

impl DataFiscal {
    /// Valida ano, mês e dia.
    pub fn new(ano: u16, mes: u8, dia: u8) -> Result<Self, ErroNfse> {
        if !(2000..=2099).contains(&ano) {
            return Err(ErroNfse::campo("data", "ano fora de 2000..=2099"));
        }
        if !(1..=12).contains(&mes) {
            return Err(ErroNfse::campo("data", "mês fora de 1..=12"));
        }
        let limite = dias_no_mes(ano, mes);
        if dia == 0 || dia > limite {
            return Err(ErroNfse::campo(
                "data",
                format!("dia {dia} inválido para {ano:04}-{mes:02}"),
            ));
        }
        Ok(Self { ano, mes, dia })
    }

    /// Lê `AAAA-MM-DD`.
    pub fn parse(entrada: &str) -> Result<Self, ErroNfse> {
        let bytes = entrada.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(ErroNfse::campo(
                "data",
                format!("formato esperado AAAA-MM-DD: {entrada:?}"),
            ));
        }
        let ano = decimal(&entrada[0..4])?;
        let mes = decimal(&entrada[5..7])?;
        let dia = decimal(&entrada[8..10])?;
        Self::new(
            u16::try_from(ano).map_err(|_| ErroNfse::campo("data", "ano inválido"))?,
            u8::try_from(mes).map_err(|_| ErroNfse::campo("data", "mês inválido"))?,
            u8::try_from(dia).map_err(|_| ErroNfse::campo("data", "dia inválido"))?,
        )
    }

    /// Data de hoje em UTC, recuada em `defasagem_segundos`.
    ///
    /// Usada por padrão em `dCompet` quando o pedido não informa a competência.
    pub fn hoje_utc(defasagem_segundos: u64) -> Result<Self, ErroNfse> {
        Self::de_unix(segundos_atuais()?.saturating_sub(defasagem_segundos))
    }

    /// Converte segundos desde `1970-01-01T00:00:00Z` em uma data UTC.
    pub fn de_unix(segundos: u64) -> Result<Self, ErroNfse> {
        let dias = i64::try_from(segundos / 86_400)
            .map_err(|_| ErroNfse::campo("data", "instante acima do intervalo"))?;
        let (ano, mes, dia) = data_de_dias(dias);
        Self::new(
            u16::try_from(ano).map_err(|_| ErroNfse::campo("data", "ano fora do intervalo"))?,
            u8::try_from(mes).map_err(|_| ErroNfse::campo("data", "mês fora do intervalo"))?,
            u8::try_from(dia).map_err(|_| ErroNfse::campo("data", "dia fora do intervalo"))?,
        )
    }

    /// Ano.
    pub fn ano(&self) -> u16 {
        self.ano
    }

    /// Mês.
    pub fn mes(&self) -> u8 {
        self.mes
    }

    /// Dia.
    pub fn dia(&self) -> u8 {
        self.dia
    }
}

impl fmt::Display for DataFiscal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.ano, self.mes, self.dia)
    }
}

/// Data e hora com fuso, no formato `AAAA-MM-DDThh:mm:ss±hh:mm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DataHoraFiscal {
    data: DataFiscal,
    hora: u8,
    minuto: u8,
    segundo: u8,
    /// Sinal do deslocamento: `-` ou `+`.
    sinal: char,
    /// Horas do deslocamento (0 a 12).
    deslocamento: u8,
}

impl DataHoraFiscal {
    /// Valida os componentes.
    pub fn new(
        data: DataFiscal,
        hora: u8,
        minuto: u8,
        segundo: u8,
        sinal: char,
        deslocamento: u8,
    ) -> Result<Self, ErroNfse> {
        if hora > 23 {
            return Err(ErroNfse::campo("dhEmi", "hora fora de 0..=23"));
        }
        if minuto > 59 {
            return Err(ErroNfse::campo("dhEmi", "minuto fora de 0..=59"));
        }
        if segundo > 59 {
            return Err(ErroNfse::campo("dhEmi", "segundo fora de 0..=59"));
        }
        if !matches!(sinal, '+' | '-') {
            return Err(ErroNfse::campo("dhEmi", "sinal de fuso deve ser + ou -"));
        }
        if deslocamento > 12 {
            return Err(ErroNfse::campo(
                "dhEmi",
                "deslocamento de fuso acima de 12:00",
            ));
        }
        if sinal == '-' && deslocamento == 12 {
            return Err(ErroNfse::campo(
                "dhEmi",
                "o leiaute só aceita -12:00 como +12:00",
            ));
        }
        Ok(Self {
            data,
            hora,
            minuto,
            segundo,
            sinal,
            deslocamento,
        })
    }

    /// Lê `AAAA-MM-DDThh:mm:ss±hh:mm`.
    pub fn parse(entrada: &str) -> Result<Self, ErroNfse> {
        let bytes = entrada.as_bytes();
        if bytes.len() != 25
            || bytes[10] != b'T'
            || bytes[13] != b':'
            || bytes[16] != b':'
            || bytes[22] != b':'
            || !matches!(bytes[19], b'+' | b'-')
        {
            return Err(ErroNfse::campo(
                "dhEmi",
                format!("formato esperado AAAA-MM-DDThh:mm:ss±hh:mm: {entrada:?}"),
            ));
        }
        let data = DataFiscal::parse(&entrada[0..10])?;
        let hora = u8::try_from(decimal(&entrada[11..13])?)
            .map_err(|_| ErroNfse::campo("dhEmi", "hora inválida"))?;
        let minuto = u8::try_from(decimal(&entrada[14..16])?)
            .map_err(|_| ErroNfse::campo("dhEmi", "minuto inválido"))?;
        let segundo = u8::try_from(decimal(&entrada[17..19])?)
            .map_err(|_| ErroNfse::campo("dhEmi", "segundo inválido"))?;
        let deslocamento = u8::try_from(decimal(&entrada[20..22])?)
            .map_err(|_| ErroNfse::campo("dhEmi", "deslocamento inválido"))?;
        if &entrada[23..25] != "00" {
            return Err(ErroNfse::campo(
                "dhEmi",
                "o leiaute aceita apenas fuso com minutos zerados",
            ));
        }
        Self::new(
            data,
            hora,
            minuto,
            segundo,
            char::from(bytes[19]),
            deslocamento,
        )
    }

    /// Data.
    pub fn data(&self) -> DataFiscal {
        self.data
    }

    /// Relógio do sistema em UTC (`+00:00`), recuado em `defasagem_segundos`.
    ///
    /// Emitir com alguns segundos de folga evita que `dhEmi` fique à frente do
    /// relógio do governo. A defasagem padrão usada pela fachada é de 60 s.
    pub fn agora_utc(defasagem_segundos: u64) -> Result<Self, ErroNfse> {
        Self::de_unix(segundos_atuais()?.saturating_sub(defasagem_segundos))
    }

    /// Converte segundos desde `1970-01-01T00:00:00Z` em data/hora UTC.
    pub fn de_unix(segundos: u64) -> Result<Self, ErroNfse> {
        let dias = i64::try_from(segundos / 86_400)
            .map_err(|_| ErroNfse::campo("dhEmi", "instante acima do intervalo"))?;
        let (ano, mes, dia) = data_de_dias(dias);
        let resto = segundos % 86_400;
        let hora = u8::try_from(resto / 3_600)
            .map_err(|_| ErroNfse::campo("dhEmi", "hora fora do intervalo"))?;
        let minuto = u8::try_from((resto % 3_600) / 60)
            .map_err(|_| ErroNfse::campo("dhEmi", "minuto fora do intervalo"))?;
        let segundo = u8::try_from(resto % 60)
            .map_err(|_| ErroNfse::campo("dhEmi", "segundo fora do intervalo"))?;
        let data = DataFiscal::new(
            u16::try_from(ano).map_err(|_| ErroNfse::campo("dhEmi", "ano fora do intervalo"))?,
            u8::try_from(mes).map_err(|_| ErroNfse::campo("dhEmi", "mês fora do intervalo"))?,
            u8::try_from(dia).map_err(|_| ErroNfse::campo("dhEmi", "dia fora do intervalo"))?,
        )?;
        Self::new(data, hora, minuto, segundo, '+', 0)
    }
}

impl fmt::Display for DataHoraFiscal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}T{:02}:{:02}:{:02}{}{:02}:00",
            self.data, self.hora, self.minuto, self.segundo, self.sinal, self.deslocamento
        )
    }
}

fn decimal(entrada: &str) -> Result<u32, ErroNfse> {
    if entrada.is_empty() || !entrada.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ErroNfse::campo(
            "data",
            format!("valor não numérico: {entrada:?}"),
        ));
    }
    entrada
        .parse()
        .map_err(|_| ErroNfse::campo("data", format!("valor fora do intervalo: {entrada:?}")))
}

fn dias_no_mes(ano: u16, mes: u8) -> u8 {
    match mes {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if bissexto(ano) => 29,
        2 => 28,
        _ => 0,
    }
}

fn bissexto(ano: u16) -> bool {
    (ano.is_multiple_of(4) && !ano.is_multiple_of(100)) || ano.is_multiple_of(400)
}

fn segundos_atuais() -> Result<u64, ErroNfse> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ErroNfse::campo("relógio", "instante anterior à época"))?
        .as_secs())
}

/// Converte dias desde 1970-01-01 em ano, mês e dia do calendário gregoriano.
///
/// Algoritmo `civil_from_days` de Howard Hinnant, sem dependência de crate de
/// calendário e sem fuso: a entrada é sempre UTC.
fn data_de_dias(dias: i64) -> (i64, u32, u32) {
    let z = dias + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let ano = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    (if mes <= 2 { ano + 1 } else { ano }, mes as u32, dia as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aceita_data_e_hora_do_leiaute() {
        assert_eq!(
            DataFiscal::parse("2026-11-08").expect("data").to_string(),
            "2026-11-08"
        );
        let dh = DataHoraFiscal::parse("2026-11-08T10:00:00-03:00").expect("data e hora");
        assert_eq!(dh.to_string(), "2026-11-08T10:00:00-03:00");
    }

    #[test]
    fn valida_bissexto_e_limites() {
        assert!(DataFiscal::new(2024, 2, 29).is_ok());
        assert!(DataFiscal::new(2025, 2, 29).is_err());
        assert!(DataFiscal::new(1999, 1, 1).is_err());
        assert!(DataFiscal::new(2026, 13, 1).is_err());
        assert!(DataFiscal::new(2026, 4, 31).is_err());
    }

    #[test]
    fn recusa_formato_fora_do_leiaute() {
        assert!(DataHoraFiscal::parse("2026-11-08T10:00:00.000-03:00").is_err());
        assert!(DataHoraFiscal::parse("2026-11-08T10:00:00-03:30").is_err());
        assert!(DataHoraFiscal::parse("2026-11-08T24:00:00-03:00").is_err());
        assert!(DataHoraFiscal::parse("2026-11-08 10:00:00-03:00").is_err());
    }

    #[test]
    fn converte_unix_para_data_e_hora_utc() {
        let dh = DataHoraFiscal::de_unix(1_794_573_296).expect("instante");
        assert_eq!(dh.to_string(), "2026-11-13T12:34:56+00:00");
        assert_eq!(
            DataHoraFiscal::de_unix(1_709_164_800)
                .expect("bissexto")
                .to_string(),
            "2024-02-29T00:00:00+00:00"
        );
        assert_eq!(
            DataHoraFiscal::de_unix(1_772_323_199)
                .expect("véspera de março")
                .to_string(),
            "2026-02-28T23:59:59+00:00"
        );
    }

    #[test]
    fn data_unix_acompanha_a_virada_do_dia() {
        assert_eq!(
            DataFiscal::de_unix(1_794_528_000)
                .expect("meia-noite")
                .to_string(),
            "2026-11-13"
        );
        assert_eq!(
            DataFiscal::de_unix(1_794_614_399)
                .expect("último segundo")
                .to_string(),
            "2026-11-13"
        );
        assert_eq!(
            DataFiscal::de_unix(1_794_614_400)
                .expect("dia seguinte")
                .to_string(),
            "2026-11-14"
        );
    }

    #[test]
    fn relogio_recua_pela_defasagem() {
        let agora = DataHoraFiscal::agora_utc(60).expect("relógio");
        let sem_defasagem = DataHoraFiscal::agora_utc(0).expect("relógio");
        assert!(agora <= sem_defasagem);
    }
}
