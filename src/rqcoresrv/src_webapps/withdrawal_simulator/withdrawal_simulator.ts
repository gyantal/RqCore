export class AnnualReturn {
  year: number = 0;
  return: number = 0; // 12 elements for January..December returns in the given year.
}

// used in PrtfVwr and ChrtGen to process seasonality matrix
export class MonthlySeasonality {
  year: number = 0;
  returns: number[] = []; // 12 elements for January..December returns in the given year.
}
// export {}; // TS convention: To avoid top level duplicate variables, functions. This file should be treated as a module (and have its own scope). A file without any top-level import or export declarations is treated as a script whose contents are available in the global scope.

export function sqAverage(arrayB: number[]): number {
  let total = 0;
  for (let i = 0; i < arrayB.length; i++)
    total += arrayB[i];
  const avg = total / arrayB.length;
  return avg;
}

export function sqMedian(arrayB: number[]): number {
  arrayB.sort(function(a: number, b: number) {
    return a - b;
  });
  const i = arrayB.length / 2;
  let med: number;
  i % 1 === 0 ? med = (arrayB[Math.floor(i) - 1] + arrayB[Math.floor(i)]) / 2 : med = arrayB[Math.floor(i)];
  return med;
}

export function sqStdDev(arrayB: number[]): number {
  const avg = sqAverage(arrayB);
  let sumdev = 0;
  for (let i = 0; i < arrayB.length; i++)
    sumdev += (arrayB[i]-avg)*(arrayB[i]-avg);
  const stdDev = Math.sqrt(sumdev/(arrayB.length-1));
  return stdDev;
}

export function sqGaussian(mean: number, stdev: number): any {
  // returns a gaussian random function with the given mean and stdev.
  let y2: number;
  let useLast: boolean = false;
  return function() {
    let y1;
    if (useLast) {
      y1 = y2;
      useLast = false;
    } else {
      let x1; let x2; let w;
      do {
        x1 = 2.0 * Math.random() - 1.0;
        x2 = 2.0 * Math.random() - 1.0;
        w = x1 * x1 + x2 * x2;
      } while (w >= 1.0);
      w = Math.sqrt((-2.0 * Math.log(w)) / w);
      y1 = x1 * w;
      y2 = x2 * w;
      useLast = true;
    }

    const retval = mean + stdev * y1;
    return retval;
  };
}

// Function to calculate average for a given number of years
export function sqAverageOfSeasonalityData(monthlySeasonality: MonthlySeasonality[], years: number): number[] {
  const avgArray: number[] = new Array(12).fill(NaN);

  for (let i = 0; i < 12; i++) {
    let returnsSum: number = 0;
    let returnsNum: number = 0; // number of items summed

    for (let j = 0; j < years; j++) { // Selecting only the data from the last 'years' years
      if (monthlySeasonality[j].returns[i] != undefined && !Number.isNaN(monthlySeasonality[j].returns[i])) {
        returnsSum += monthlySeasonality[j].returns[i];
        returnsNum++;
      }
    }
    avgArray[i] = returnsSum / returnsNum;
  }
  return avgArray;
}

export function sqAverageOfAnnualReturns(annualReturns: AnnualReturn[], years: number): number {
  let avgRet = 0;
  let returnsSum: number = 0;
  let returnsNum: number = 0; // number of items summed

  for (let j = 0; j < years; j++) { // Selecting only the data from the last 'years' years
    if (annualReturns[j].return != undefined && !Number.isNaN(annualReturns[j].return)) {
      returnsSum += annualReturns[j].return;
      returnsNum++;
    }
  }
  avgRet = returnsSum/returnsNum;
  return avgRet;
}

// 1. Declare some global variables and hook on DOMContentLoaded() and window.onload()
console.log('SqCore: Script BEGIN4');

function withdrawalSimulationDriver() {
  console.log('simulationDriverFunction() BEGIN2222');

  const simLengthHtmlElement = document.getElementById('idSimLength') as HTMLInputElement;
  let simLength = Number(simLengthHtmlElement.value);
  if (simLength > 50)
    simLength = 50;
    // idSimLength.value = 50;

  const cagr = Number((document.getElementById('idCAGR') as HTMLInputElement).value);
  const stdDev = Number((document.getElementById('idStdDev') as HTMLInputElement).value);
  const nSimulations = Number((document.getElementById('idnSimulations') as HTMLInputElement).value);
  const iniDep = Number((document.getElementById('idIniDep') as HTMLInputElement).value);
  let addDepFreq = Number((document.getElementById('idAddDepFreq') as HTMLInputElement).value);
  const addDepVal = Number((document.getElementById('idAddDepVal') as HTMLInputElement).value);
  const addDepEnd = (document.getElementById('idAddDepEnd') as HTMLInputElement).value ? Number((document.getElementById('idAddDepEnd') as HTMLInputElement).value) : simLength * 12;
  let withDrawStart = Number((document.getElementById('idWithDrawStart') as HTMLInputElement).value);
  const withDrawFreq = Number((document.getElementById('idWithDrawFreq') as HTMLInputElement).value);
  const withDrawPerc = Number((document.getElementById('idWithDrawPerc') as HTMLInputElement).value);

  if (addDepFreq == 0) {
    if (addDepVal > 0)
      addDepFreq = 12;
    else
      addDepFreq = 12;
  }

  if (addDepFreq > 60)
    addDepFreq = 60;

  if (withDrawStart > 600)
    withDrawStart = 600;

  const userErrorHtmlElement = document.getElementById('idUserError') as HTMLElement;
  const isError = ((withDrawFreq + withDrawPerc + withDrawStart > 0 && withDrawFreq * withDrawPerc * withDrawStart == 0) || withDrawPerc < 0 || withDrawFreq % 1 !== 0 || withDrawFreq < 0 || withDrawStart % 1 !== 0 || withDrawStart < 0 || addDepEnd > simLength * 12 || addDepEnd % 1 !== 0 || addDepEnd < 0 || addDepVal < 0 || addDepFreq % 1 !== 0 || addDepFreq < 0 || iniDep < 0 || stdDev <= 0 || nSimulations < 1 || nSimulations % 1 !== 0 || simLength % 1 !== 0 || isNaN(simLength) || isNaN(cagr) || isNaN(stdDev) || isNaN(nSimulations) || isNaN(iniDep) || isNaN(addDepFreq) || isNaN(addDepVal) || isNaN(addDepEnd) || isNaN(withDrawStart) || isNaN(withDrawFreq) || isNaN(withDrawPerc)) ? true : false;
  if (isError != false) {
    userErrorHtmlElement.innerText = 'ERROR in inputs';
    userErrorHtmlElement.style.color = 'red';
  } else {
    userErrorHtmlElement.innerText = '';
    userErrorHtmlElement.style.color = 'black';

    console.log('simulationDriverFunction() inputs OK.');

    withdrawalSimulation(simLength, cagr, stdDev, nSimulations, iniDep, addDepFreq, addDepVal, addDepEnd, withDrawStart, withDrawFreq, withDrawPerc);
  }


  console.log('simulateFunction() END');
}

function withdrawalSimulation(simLength: number, cagr: number, stdDev: number, nSimulations: number, iniDep: number, addDepFreq: number, addDepVal: number, addDepEnd: number, withDrawStart: number, withDrawFreq: number, withDrawPerc: number): void {
  console.log('simulateFunction() BEGIN');

  //  TODO: calculate
  const totalDeposit = iniDep + Math.floor(addDepEnd / addDepFreq) * addDepVal;

  const developerInfo = '';

  // TODO: write into developer Info


  // make a standard gaussian variable.
  const standard = sqGaussian(cagr / 100 / 252, stdDev / 100 / Math.sqrt(252));

  const nDays = simLength * 252;

  const dailyRets = new Array(nSimulations);
  for (let i = 0; i < dailyRets.length; i++) {
    const dailyRet = new Array(nDays);
    for (let j = 0; j < dailyRet.length; j++)
      dailyRet[j] = standard();
    dailyRets[i] = dailyRet;
  }

  const dailyPrices = new Array(nSimulations);
  for (let i = 0; i < dailyPrices.length; i++) {
    const dailyPrice = new Array(nDays);
    dailyPrice[0] = 1 + dailyRets[i][0];
    for (let j = 1; j < dailyPrice.length; j++)
      dailyPrice[j] = dailyPrice[j - 1] * (1 + dailyRets[i][j]);
    dailyPrices[i] = dailyPrice;
  }

  const dailyAddDep = new Array(nDays);
  for (let i = 0; i < dailyAddDep.length; i++) {
    if (i < addDepEnd * 21 && i % (21 * addDepFreq) == (21 * addDepFreq - 1))
      dailyAddDep[i] = addDepVal;
    else
      dailyAddDep[i] = 0;
  }

  const dailyWithDraw = new Array(nDays);
  for (let i = 0; i < dailyWithDraw.length; i++) {
    if (i >= (withDrawStart * 21 - 1) && i % (21 * withDrawFreq) == (21 * withDrawFreq - 1))
      dailyWithDraw[i] = withDrawPerc / 100;
    else
      dailyWithDraw[i] = 0;
  }

  const dailyNoShs = new Array(nSimulations);
  const dailyWithPVs = new Array(nSimulations);
  for (let i = 0; i < dailyNoShs.length; i++) {
    const dailyNoSh = new Array(nDays);
    const dailyWithPV = new Array(nDays);
    dailyNoSh[0] = iniDep;
    dailyWithPV[0] = 0;
    for (let j = 1; j < dailyNoSh.length; j++) {
      dailyNoSh[j] = dailyNoSh[j - 1] + dailyAddDep[j] / dailyPrices[i][j] - dailyNoSh[j - 1] * dailyWithDraw[j];
      dailyWithPV[j] = dailyWithPV[j - 1] + dailyNoSh[j - 1] * dailyWithDraw[j] * dailyPrices[i][j];
    }

    dailyNoShs[i] = dailyNoSh;
    dailyWithPVs[i] = dailyWithPV;
  }

  const dailyNoShsBH = new Array(nSimulations);
  for (let i = 0; i < dailyNoShsBH.length; i++) {
    const dailyNoShBH = new Array(nDays);
    dailyNoShBH[0] = iniDep;
    for (let j = 1; j < dailyNoShBH.length; j++)
      dailyNoShBH[j] = dailyNoShBH[j - 1] + dailyAddDep[j] / dailyPrices[i][j];

    dailyNoShsBH[i] = dailyNoShBH;
  }

  const dailyPVs = new Array(nSimulations);
  for (let i = 0; i < dailyPVs.length; i++) {
    const dailyPV = new Array(nDays);
    for (let j = 0; j < dailyPV.length; j++)
      dailyPV[j] = dailyNoShs[i][j] * dailyPrices[i][j];
    dailyPVs[i] = dailyPV;
  }

  const dailyPVsBH = new Array(nSimulations);
  for (let i = 0; i < dailyPVsBH.length; i++) {
    const dailyPVBH = new Array(nDays);
    for (let j = 0; j < dailyPVBH.length; j++)
      dailyPVBH[j] = dailyNoShsBH[i][j] * dailyPrices[i][j];
    dailyPVsBH[i] = dailyPVBH;
  }

  withdrawalSimulationResult(simLength, nDays, nSimulations, dailyPVs, dailyPVsBH, dailyWithPVs, totalDeposit, developerInfo);
}

function withdrawalSimulationResult(simLength: number, nDays: number, nSimulations: number, dailyPVs: number[][], dailyPVsBH: number[][], dailyWithPVs: number[][], totalDeposit: number, developerInfo: string) {
  const pvAvgWoWith = new Array(nDays);
  const pvMedWoWith = new Array(nDays);
  for (let i = 0; i < pvAvgWoWith.length; i++) {
    const subPVAvgWoWith = new Array(nSimulations);
    for (let j = 0; j < subPVAvgWoWith.length; j++)
      subPVAvgWoWith[j] = dailyPVsBH[j][i];

    pvAvgWoWith[i] = sqAverage(subPVAvgWoWith);
    pvMedWoWith[i] = sqMedian(subPVAvgWoWith);
  }

  const pvRndWoWith = dailyPVsBH[0];

  const pvAvgWith = new Array(nDays);
  const pvMedWith = new Array(nDays);
  for (let i = 0; i < pvAvgWith.length; i++) {
    const subPVAvgWith = new Array(nSimulations);
    for (let j = 0; j < subPVAvgWith.length; j++)
      subPVAvgWith[j] = dailyPVs[j][i];
    pvAvgWith[i] = sqAverage(subPVAvgWith);
    pvMedWith[i] = sqMedian(subPVAvgWith);
  }

  const pvRndWith = dailyPVs[0];

  const pvAvgWithDrawed = new Array(nDays);
  const pvMedWithDrawed = new Array(nDays);
  for (let i = 0; i < pvAvgWithDrawed.length; i++) {
    const subPVAvgWithDrawed = new Array(nSimulations);
    for (let j = 0; j < subPVAvgWithDrawed.length; j++)
      subPVAvgWithDrawed[j] = dailyWithPVs[j][i];
    pvAvgWithDrawed[i] = sqAverage(subPVAvgWithDrawed);
    pvMedWithDrawed[i] = sqMedian(subPVAvgWithDrawed);
  }

  const pvRndWithDrawed = dailyWithPVs[0];

  const pvAvgTotal = new Array(nDays);
  const pvMedTotal = new Array(nDays);
  const pvRndTotal = new Array(nDays);
  for (let i = 0; i < pvAvgTotal.length; i++) {
    pvAvgTotal[i] = pvAvgWith[i] + pvAvgWithDrawed[i];
    pvMedTotal[i] = pvMedWith[i] + pvMedWithDrawed[i];
    pvRndTotal[i] = pvRndWith[i] + pvRndWithDrawed[i];
  }

  const profitDollAvgWoWith = pvAvgWoWith[nDays - 1] - totalDeposit;
  const profitPercAvgWoWith = pvAvgWoWith[nDays - 1] / totalDeposit - 1;
  const cagrAvgWoWith = Math.pow((pvAvgWoWith[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollMedWoWith = pvMedWoWith[nDays - 1] - totalDeposit;
  const profitPercMedWoWith = pvMedWoWith[nDays - 1] / totalDeposit - 1;
  const cagrMedWoWith = Math.pow((pvMedWoWith[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollRndWoWith = pvRndWoWith[nDays - 1] - totalDeposit;
  const profitPercRndWoWith = pvRndWoWith[nDays - 1] / totalDeposit - 1;
  const cagrRndWoWith = Math.pow((pvRndWoWith[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollAvgWith = pvAvgWith[nDays - 1] - totalDeposit;
  const profitPercAvgWith = pvAvgWith[nDays - 1] / totalDeposit - 1;
  const cagrAvgWith = Math.pow((pvAvgWith[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollMedWith = pvMedWith[nDays - 1] - totalDeposit;
  const profitPercMedWith = pvMedWith[nDays - 1] / totalDeposit - 1;
  const cagrMedWith = Math.pow((pvMedWith[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollRndWith = pvRndWith[nDays - 1] - totalDeposit;
  const profitPercRndWith = pvRndWith[nDays - 1] / totalDeposit - 1;
  const cagrRndWith = Math.pow((pvRndWith[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollAvgWithDrawed = pvAvgWithDrawed[nDays - 1];
  const profitPercAvgWithDrawed = pvAvgWithDrawed[nDays - 1] / totalDeposit;
  const cagrAvgWithDrawed = Math.pow(1 + (pvAvgWithDrawed[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollMedWithDrawed = pvMedWithDrawed[nDays - 1];
  const profitPercMedWithDrawed = pvMedWithDrawed[nDays - 1] / totalDeposit;
  const cagrMedWithDrawed = Math.pow(1 + (pvMedWithDrawed[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollRndWithDrawed = pvRndWithDrawed[nDays - 1];
  const profitPercRndWithDrawed = pvRndWithDrawed[nDays - 1] / totalDeposit;
  const cagrRndWithDrawed = Math.pow(1 + (pvRndWithDrawed[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollAvgTotal = pvAvgTotal[nDays - 1] - totalDeposit;
  const profitPercAvgTotal = pvAvgTotal[nDays - 1] / totalDeposit - 1;
  const cagrAvgTotal = Math.pow((pvAvgTotal[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollMedTotal = pvMedTotal[nDays - 1] - totalDeposit;
  const profitPercMedTotal = pvMedTotal[nDays - 1] / totalDeposit - 1;
  const cagrMedTotal = Math.pow((pvMedTotal[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const profitDollRndTotal = pvRndTotal[nDays - 1] - totalDeposit;
  const profitPercRndTotal = pvRndTotal[nDays - 1] / totalDeposit - 1;
  const cagrRndTotal = Math.pow((pvRndTotal[nDays - 1] / totalDeposit), 1 / simLength) - 1;

  const proba = [1, 1, 1, 1, 1, 1];
  const averagePVWoWith = sqStdDev(proba);

  withdrawalSimulatorOutput(totalDeposit, developerInfo, averagePVWoWith, profitDollAvgWoWith, profitPercAvgWoWith, profitDollMedWoWith, profitPercMedWoWith, profitDollRndWoWith, profitPercRndWoWith, cagrAvgWoWith, cagrMedWoWith, cagrRndWoWith, profitDollAvgWith, profitPercAvgWith, profitDollMedWith, profitPercMedWith, profitDollRndWith, profitPercRndWith, cagrAvgWith, cagrMedWith, cagrRndWith, profitDollAvgWithDrawed, profitPercAvgWithDrawed, profitDollMedWithDrawed, profitPercMedWithDrawed, profitDollRndWithDrawed, profitPercRndWithDrawed, cagrAvgWithDrawed, cagrMedWithDrawed, cagrRndWithDrawed, profitDollAvgTotal, profitPercAvgTotal, profitDollMedTotal, profitPercMedTotal, profitDollRndTotal, profitPercRndTotal, cagrAvgTotal, cagrMedTotal, cagrRndTotal);
  console.log('simulateFunction() END');
}

function withdrawalSimulatorOutput(totalDeposit: number, developerInfo: string, averagePVWoWith: number, profitDollAvgWoWith: number, profitPercAvgWoWith: number, profitDollMedWoWith: number, profitPercMedWoWith: number, profitDollRndWoWith: number, profitPercRndWoWith: number, cagrAvgWoWith: number, cagrMedWoWith: number, cagrRndWoWith: number, profitDollAvgWith: number, profitPercAvgWith: number, profitDollMedWith: number, profitPercMedWith: number, profitDollRndWith: number, profitPercRndWith: number, cagrAvgWith: number, cagrMedWith: number, cagrRndWith: number, profitDollAvgWithDrawed: number, profitPercAvgWithDrawed: number, profitDollMedWithDrawed: number, profitPercMedWithDrawed: number, profitDollRndWithDrawed: number, profitPercRndWithDrawed: number, cagrAvgWithDrawed: number, cagrMedWithDrawed: number, cagrRndWithDrawed: number, profitDollAvgTotal: number, profitPercAvgTotal: number, profitDollMedTotal: number, profitPercMedTotal: number, profitDollRndTotal: number, profitPercRndTotal: number, cagrAvgTotal: number, cagrMedTotal: number, cagrRndTotal: number) {
  const totalDepositElem = document.getElementById('idTotalDeposit') as HTMLElement;
  totalDepositElem.innerText = totalDeposit.toLocaleString('en-US') + ' $';

  const profitDollAvgWoWithElem = document.getElementById('idProfitDollAvgWoWith') as HTMLElement;
  profitDollAvgWoWithElem.innerText = Math.floor(profitDollAvgWoWith).toLocaleString('en-US') + ' $';

  const profitDollMedWoWithElem = document.getElementById('idProfitDollMedWoWith') as HTMLElement;
  profitDollMedWoWithElem.innerText = Math.floor(profitDollMedWoWith).toLocaleString('en-US') + ' $';

  const profitDollRndWoWithElem = document.getElementById('idProfitDollRndWoWith') as HTMLElement;
  profitDollRndWoWithElem.innerText = Math.floor(profitDollRndWoWith).toLocaleString('en-US') + ' $';

  const profitPercAvgWoWithElem = document.getElementById('idProfitPercAvgWoWith') as HTMLElement;
  profitPercAvgWoWithElem.innerText = (Math.floor(profitPercAvgWoWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitPercMedWoWithElem = document.getElementById('idProfitPercMedWoWith') as HTMLElement;
  profitPercMedWoWithElem.innerText = (Math.floor(profitPercMedWoWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitPercRndWoWithElem = document.getElementById('idProfitPercRndWoWith') as HTMLElement;
  profitPercRndWoWithElem.innerText = (Math.floor(profitPercRndWoWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRAvgWoWithElem = document.getElementById('idCAGRAvgWoWith') as HTMLElement;
  profitCAGRAvgWoWithElem.innerText = (Math.floor(cagrAvgWoWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRMedWoWithElem = document.getElementById('idCAGRMedWoWith') as HTMLElement;
  profitCAGRMedWoWithElem.innerText = (Math.floor(cagrMedWoWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRRndWoWithElem = document.getElementById('idCAGRRndWoWith') as HTMLElement;
  profitCAGRRndWoWithElem.innerText = (Math.floor(cagrRndWoWith * 1000) / 10).toLocaleString('en-US') + ' %';

  // var profitSharpeAvgWoWithHtmlElement = document.getElementById("idSharpeAvgWoWith");
  // profitSharpeAvgWoWithHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  // var profitSharpeMedWoWithHtmlElement = document.getElementById("idSharpeMedWoWith");
  // profitSharpeMedWoWithHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  // var profitSharpeRndWoWithHtmlElement = document.getElementById("idSharpeRndWoWith");
  // profitSharpeRndWoWithHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;


  const profitDollAvgWithDrawedElem = document.getElementById('idProfitDollAvgWithDrawed') as HTMLElement;
  profitDollAvgWithDrawedElem.innerText = Math.floor(profitDollAvgWithDrawed).toLocaleString('en-US') + ' $';

  const profitDollMedWithDrawedElem = document.getElementById('idProfitDollMedWithDrawed') as HTMLElement;
  profitDollMedWithDrawedElem.innerText = Math.floor(profitDollMedWithDrawed).toLocaleString('en-US') + ' $';

  const profitDollRndWithDrawedElem = document.getElementById('idProfitDollRndWithDrawed') as HTMLElement;
  profitDollRndWithDrawedElem.innerText = Math.floor(profitDollRndWithDrawed).toLocaleString('en-US') + ' $';

  const profitPercAvgWithDrawedElem = document.getElementById('idProfitPercAvgWithDrawed') as HTMLElement;
  profitPercAvgWithDrawedElem.innerText = (Math.floor(profitPercAvgWithDrawed * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitPercMedWithDrawedElem = document.getElementById('idProfitPercMedWithDrawed') as HTMLElement;
  profitPercMedWithDrawedElem.innerText = (Math.floor(profitPercMedWithDrawed * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitPercRndWithDrawedElem = document.getElementById('idProfitPercRndWithDrawed') as HTMLElement;
  profitPercRndWithDrawedElem.innerText = (Math.floor(profitPercRndWithDrawed * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRAvgWithDrawedElem = document.getElementById('idCAGRAvgWithDrawed') as HTMLElement;
  profitCAGRAvgWithDrawedElem.innerText = (Math.floor(cagrAvgWithDrawed * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRMedWithDrawedElem = document.getElementById('idCAGRMedWithDrawed') as HTMLElement;
  profitCAGRMedWithDrawedElem.innerText = (Math.floor(cagrMedWithDrawed * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRRndWithDrawedElem = document.getElementById('idCAGRRndWithDrawed') as HTMLElement;
  profitCAGRRndWithDrawedElem.innerText = (Math.floor(cagrRndWithDrawed * 1000) / 10).toLocaleString('en-US') + ' %';

  // var profitSharpeAvgWithDrawedHtmlElement = document.getElementById("idSharpeAvgWithDrawed");
  // profitSharpeAvgWithDrawedHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  // var profitSharpeMedWithDrawedHtmlElement = document.getElementById("idSharpeMedWithDrawed");
  // profitSharpeMedWithDrawedHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  // var profitSharpeRndWithDrawedHtmlElement = document.getElementById("idSharpeRndWithDrawed");
  // profitSharpeRndWithDrawedHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  const profitDollAvgStillInvElem = document.getElementById('idProfitDollAvgStillInv') as HTMLElement;
  profitDollAvgStillInvElem.innerText = Math.floor(profitDollAvgWith).toLocaleString('en-US') + ' $';

  const profitDollMedStillInvElem = document.getElementById('idProfitDollMedStillInv') as HTMLElement;
  profitDollMedStillInvElem.innerText = Math.floor(profitDollMedWith).toLocaleString('en-US') + ' $';

  const profitDollRndStillInvElem = document.getElementById('idProfitDollRndStillInv') as HTMLElement;
  profitDollRndStillInvElem.innerText = Math.floor(profitDollRndWith).toLocaleString('en-US') + ' $';

  const profitPercAvgStillInvElem = document.getElementById('idProfitPercAvgStillInv') as HTMLElement;
  profitPercAvgStillInvElem.innerText = (Math.floor(profitPercAvgWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitPercMedStillInvElem = document.getElementById('idProfitPercMedStillInv') as HTMLElement;
  profitPercMedStillInvElem.innerText = (Math.floor(profitPercMedWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitPercRndStillInvElem = document.getElementById('idProfitPercRndStillInv') as HTMLElement;
  profitPercRndStillInvElem.innerText = (Math.floor(profitPercRndWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRAvgStillInvElem = document.getElementById('idCAGRAvgStillInv') as HTMLElement;
  profitCAGRAvgStillInvElem.innerText = (Math.floor(cagrAvgWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRMedStillInvElem = document.getElementById('idCAGRMedStillInv') as HTMLElement;
  profitCAGRMedStillInvElem.innerText = (Math.floor(cagrMedWith * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRRndStillInvElem = document.getElementById('idCAGRRndStillInv') as HTMLElement;
  profitCAGRRndStillInvElem.innerText = (Math.floor(cagrRndWith * 1000) / 10).toLocaleString('en-US') + ' %';

  // var profitSharpeAvgStillInvHtmlElement = document.getElementById("idSharpeAvgStillInv");
  // profitSharpeAvgStillInvHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  // var profitSharpeMedStillInvHtmlElement = document.getElementById("idSharpeMedStillInv");
  // profitSharpeMedStillInvHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  // var profitSharpeRndStillInvHtmlElement = document.getElementById("idSharpeRndStillInv");
  // profitSharpeRndStillInvHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  const profitDollAvgTotalElem = document.getElementById('idProfitDollAvgTotal') as HTMLElement;
  profitDollAvgTotalElem.innerText = Math.floor(profitDollAvgTotal).toLocaleString('en-US') + ' $';

  const profitDollMedTotalElem = document.getElementById('idProfitDollMedTotal') as HTMLElement;
  profitDollMedTotalElem.innerText = Math.floor(profitDollMedTotal).toLocaleString('en-US') + ' $';

  const profitDollRndTotalElem = document.getElementById('idProfitDollRndTotal') as HTMLElement;
  profitDollRndTotalElem.innerText = Math.floor(profitDollRndTotal).toLocaleString('en-US') + ' $';

  const profitPercAvgTotalElem = document.getElementById('idProfitPercAvgTotal') as HTMLElement;
  profitPercAvgTotalElem.innerText = (Math.floor(profitPercAvgTotal * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitPercMedTotalElem = document.getElementById('idProfitPercMedTotal') as HTMLElement;
  profitPercMedTotalElem.innerText = (Math.floor(profitPercMedTotal * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitPercRndTotalElem = document.getElementById('idProfitPercRndTotal') as HTMLElement;
  profitPercRndTotalElem.innerText = (Math.floor(profitPercRndTotal * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRAvgTotalElem = document.getElementById('idCAGRAvgTotal') as HTMLElement;
  profitCAGRAvgTotalElem.innerText = (Math.floor(cagrAvgTotal * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRMedTotalElem = document.getElementById('idCAGRMedTotal') as HTMLElement;
  profitCAGRMedTotalElem.innerText = (Math.floor(cagrMedTotal * 1000) / 10).toLocaleString('en-US') + ' %';

  const profitCAGRRndTotalElem = document.getElementById('idCAGRRndTotal') as HTMLElement;
  profitCAGRRndTotalElem.innerText = (Math.floor(cagrRndTotal * 1000) / 10).toLocaleString('en-US') + ' %';

  // var profitSharpeAvgTotalHtmlElement = document.getElementById("idSharpeAvgTotal");
  // profitSharpeAvgTotalHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  // var profitSharpeMedTotalHtmlElement = document.getElementById("idSharpeMedTotal");
  // profitSharpeMedTotalHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;

  // var profitSharpeRndTotalHtmlElement = document.getElementById("idSharpeRndTotal");
  // profitSharpeRndTotalHtmlElement.innerText = Math.floor(averagePVWoWith * 100) / 100;


  const developerInfoElem = document.getElementById('idDeveloperInfo') as HTMLElement;
  developerInfoElem.innerText = developerInfo;
}

const simulateBtn = document.getElementById('simulate') as HTMLElement;
simulateBtn.onclick = function() { withdrawalSimulationDriver(); };

console.log('Body is running...');

console.log('SqCore: Script END');