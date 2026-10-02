export type EdaEdition = 'easyeda' | 'jlceda';
export type EdaApiOptions = { edaEdition?: EdaEdition };

export function getEdaApiBase(edition: EdaEdition = 'easyeda') {
    return edition === 'jlceda' ? 'https://pro.lceda.cn' : 'https://pro.easyeda.com';
}
