//! Vetores de canonicalização C14N 1.0 (adaptados para XML sem DTD).

use carimbar_xml::Documento;

fn canonico(xml: &str) -> String {
    Documento::parse(xml.as_bytes())
        .expect("XML válido")
        .canonicalizar()
        .expect("canonicaliza")
}

#[test]
fn vetor_3_1_pis_comentarios_e_fora_da_raiz() {
    let entrada = "<?xml version=\"1.0\"?>\n\
<?xml-stylesheet   href=\"doc.xsl\"\n   type=\"text/xsl\"   ?>\n\
\n\
<doc>Hello, world!<!-- Comment 1 --></doc>\n\
\n\
<?pi-without-data     ?>\n\
\n\
<!-- Comment 2 -->\n";
    let esperado = "<?xml-stylesheet href=\"doc.xsl\"\n   \
type=\"text/xsl\"   ?>\n\
<doc>Hello, world!</doc>\n\
<?pi-without-data?>";
    assert_eq!(canonico(entrada), esperado);
}

#[test]
fn vetor_3_3_tags_inicio_e_fim() {
    let entrada = r#"<doc>
   <e1   />
   <e2   ></e2>
   <e3   name = "elem3"   id="elem3"   />
   <e4   name="elem4"   id="elem4"   ></e4>
   <e5 a:attr="out" b:attr="sorted" attr2="all" attr="I'm"
      xmlns:b="http://www.ietf.org"
      xmlns:a="http://www.w3.org"
      xmlns="http://example.org"/>
   <e6 xmlns=""
      xmlns:a="http://www.w3.org">
      <e7
         xmlns="http://www.ietf.org">
         <e8
            xmlns="" xmlns:a="http://www.w3.org">
            <e9
               xmlns="" xmlns:a="http://www.ietf.org"/>
         </e8>
      </e7>
   </e6>
</doc>"#;
    let esperado = r#"<doc>
   <e1></e1>
   <e2></e2>
   <e3 id="elem3" name="elem3"></e3>
   <e4 id="elem4" name="elem4"></e4>
   <e5 xmlns="http://example.org" xmlns:a="http://www.w3.org" xmlns:b="http://www.ietf.org" attr="I'm" attr2="all" b:attr="sorted" a:attr="out"></e5>
   <e6 xmlns:a="http://www.w3.org">
      <e7 xmlns="http://www.ietf.org">
         <e8 xmlns="">
            <e9 xmlns:a="http://www.ietf.org"></e9>
         </e8>
      </e7>
   </e6>
</doc>"#;
    assert_eq!(canonico(entrada), esperado);
}

#[test]
fn vetor_3_4_caracteres_e_referencias() {
    let entrada = "<doc><text>First line&#x0d;&#10;Second\nline</text>\
<value>&#x32;</value>\
<compute><![CDATA[value>\"0\" && value<\"10\" ?\"valid\":\"error\"]]></compute>\
<compute expr='value>\"0\" &amp;&amp; value&lt;\"10\" ?\"valid\":\"error\"'>valid</compute>\
</doc>";
    let esperado = "<doc><text>First line&#xD;\nSecond\nline</text>\
<value>2</value>\
<compute>value&gt;\"0\" &amp;&amp; value&lt;\"10\" ?\"valid\":\"error\"</compute>\
<compute expr=\"value>&quot;0&quot; &amp;&amp; value&lt;&quot;10&quot; ?&quot;valid&quot;:&quot;error&quot;\">valid</compute>\
</doc>";
    assert_eq!(canonico(entrada), esperado);
}

#[test]
fn remove_declaracao_e_comentarios() {
    let entrada = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!--a--><r><!--b--><x/></r><!--c-->";
    assert_eq!(canonico(entrada), "<r><x></x></r>");
}

#[test]
fn prefixo_xml_nao_e_declarado() {
    let entrada = r#"<r xml:lang="pt" xmlns="urn:x"><a xmlns=""/></r>"#;
    assert_eq!(
        canonico(entrada),
        r#"<r xmlns="urn:x" xml:lang="pt"><a xmlns=""></a></r>"#
    );
}

#[test]
fn ordena_namespaces_e_atributos() {
    let entrada = r#"<r xmlns:z="urn:z" xmlns:a="urn:a" z:x="1" a:x="2" b="0"/>"#;
    assert_eq!(
        canonico(entrada),
        r#"<r xmlns:a="urn:a" xmlns:z="urn:z" b="0" a:x="2" z:x="1"></r>"#
    );
}

#[test]
fn canonicalizacao_e_idempotente() {
    let entrada = r#"<r xmlns="urn:x" xmlns:p="urn:p"><p:a p:x="1 &amp; 2"><b/></p:a></r>"#;
    let primeira = canonico(entrada);
    let segunda = canonico(&primeira);
    assert_eq!(primeira, segunda);
}

#[test]
fn escapa_cr_em_atributo() {
    let entrada = "<r a=\"x&#xD;y\"/>";
    assert_eq!(canonico(entrada), "<r a=\"x&#xD;y\"></r>");
}

#[test]
fn subarvore_herda_namespaces_e_xml_lang_do_ancestral() {
    let documento = Documento::parse(
        br#"<r xmlns="urn:a" xmlns:p="urn:p" xml:lang="pt"><p:x a="1"><y/></p:x></r>"#,
    )
    .expect("XML válido");
    assert_eq!(
        documento.canonicalizar_subarvore(&[0]).expect("subárvore"),
        r#"<p:x xmlns="urn:a" xmlns:p="urn:p" a="1" xml:lang="pt"><y></y></p:x>"#
    );
}

#[test]
fn subarvore_nao_herda_xml_lang_quando_o_proprio_elemento_o_declara() {
    let documento =
        Documento::parse(br#"<r xml:lang="pt"><x xml:lang="en"/></r>"#).expect("XML válido");
    assert_eq!(
        documento.canonicalizar_subarvore(&[0]).expect("subárvore"),
        r#"<x xml:lang="en"></x>"#
    );
}

#[test]
fn c14n_10_herda_atributos_xml_do_ancestral_mais_proximo() {
    let documento = Documento::parse(br#"<r xml:base="https://exemplo.invalid/" xml:id="raiz" xml:space="preserve"><x xml:base="relativo/"><y/></x></r>"#).expect("XML válido");
    assert_eq!(
        documento
            .canonicalizar_subarvore(&[0, 0])
            .expect("subárvore"),
        r#"<y xml:base="relativo/" xml:id="raiz" xml:space="preserve"></y>"#,
    );
}

#[test]
fn caminhos_por_atributo_encontra_duplicatas() {
    let documento = Documento::parse(br#"<r><a Id="1"/><b><c Id="1"/></b><d Id="2"/></r>"#)
        .expect("XML válido");
    assert_eq!(documento.caminhos_por_atributo("Id", "1").len(), 2);
    assert_eq!(documento.caminhos_por_atributo("Id", "2"), vec![vec![2]]);
    assert!(documento.caminhos_por_atributo("Id", "3").is_empty());
    assert!(documento.elemento_em(&[9]).is_none());
}

#[test]
fn namespace_do_elemento_resolve_prefixos_dos_ancestrais() {
    let documento =
        Documento::parse(br#"<r xmlns="urn:a" xmlns:p="urn:p"><p:x><y/></p:x><z xmlns=""/></r>"#)
            .expect("XML válido");
    assert_eq!(
        documento.namespace_do_elemento(&[]).as_deref(),
        Some("urn:a")
    );
    assert_eq!(
        documento.namespace_do_elemento(&[0]).as_deref(),
        Some("urn:p")
    );
    assert_eq!(
        documento.namespace_do_elemento(&[0, 0]).as_deref(),
        Some("urn:a")
    );
    assert_eq!(documento.namespace_do_elemento(&[1]).as_deref(), Some(""));
    assert!(documento.namespace_do_elemento(&[7]).is_none());
    assert_eq!(documento.caminhos_por_local("x"), vec![vec![0]]);
    let com_prefixo_xml = Documento::parse(b"<r><xml:x/></r>").expect("XML válido");
    assert_eq!(
        com_prefixo_xml.namespace_do_elemento(&[0]).as_deref(),
        Some(carimbar_xml::URI_XML),
    );
}
